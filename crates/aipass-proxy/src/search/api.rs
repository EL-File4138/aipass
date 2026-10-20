//! Search API transports. Credentials never follow redirects or appear in errors.
use super::*;
pub(super) async fn search(state: &RuntimeState, query: &str) -> Result<Value, String> {
    if query.trim().is_empty() || query.len() > 4096 {
        return Err("search query must contain 1–4096 bytes".into());
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    let apis = state
        .config
        .read()
        .map_err(|_| "search configuration unavailable")?
        .search_apis
        .clone();
    if apis.is_empty() {
        return Err("configure a search API or a search model in local proxy settings".into());
    }
    let client = upstream_client_for_transport(state, 10_000, false)?;
    for api in apis {
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        let attempt = (tokio::time::Instant::now() + Duration::from_secs(30)).min(deadline);
        let result = tokio::time::timeout_at(attempt, ask(&client, &api, query)).await;
        if let Ok(Ok(value)) = result {
            state.usage.log_diagnostic(
                "info",
                format!("event=proxy.search.completed source_id={}", api.config.id),
            );
            return Ok(value);
        }
        state.usage.log_diagnostic(
            "warn",
            format!("event=proxy.search.failed source_id={}", api.config.id),
        );
    }
    Err("all configured search APIs failed; check credentials, allowance and connectivity".into())
}
async fn ask(
    client: &reqwest::Client,
    api: &ResolvedSearchApi,
    query: &str,
) -> Result<Value, String> {
    let base = api
        .config
        .base_url
        .as_deref()
        .unwrap_or(api.config.vendor.base())
        .trim_end_matches('/');
    let post = |path: &str, value: Value| client.post(format!("{base}{path}")).json(&value);
    let get = |path: &str, pairs: &[(&str, &str)]| -> Result<reqwest::RequestBuilder, String> {
        let mut url =
            reqwest::Url::parse(&format!("{base}{path}")).map_err(|_| "invalid search API URL")?;
        url.query_pairs_mut().extend_pairs(pairs.iter().copied());
        Ok(client.get(url))
    };
    let mut request = match api.config.vendor {
        SearchVendor::Tavily => post(
            "/search",
            json!({"query":query,"max_results":6,"search_depth":"basic"}),
        ),
        SearchVendor::Brave => get("/res/v1/web/search", &[("q", query), ("count", "6")])?,
        SearchVendor::Exa => post(
            "/search",
            json!({"query":query,"numResults":6,"contents":{"text":{"maxCharacters":4000}}}),
        ),
        SearchVendor::Firecrawl => post("/v2/search", json!({"query":query,"limit":6})),
        SearchVendor::You => post("/v1/search", json!({"query":query,"count":6})),
        SearchVendor::Deepseek => {
            if query.chars().count() > 128 {
                return Err("DeepSeek search accepts at most 128 characters".into());
            }
            post("/v1/tools/search", json!({"query":query,"count":6}))
        }
        SearchVendor::Searxng => get("/search", &[("q", query), ("format", "json")])?,
    };
    if !api.api_key.is_empty() {
        request = match api.config.vendor {
            SearchVendor::Brave => request.header("X-Subscription-Token", &api.api_key),
            SearchVendor::Exa | SearchVendor::You => request.header("X-API-Key", &api.api_key),
            _ => request.bearer_auth(&api.api_key),
        };
    }
    let response = request
        .header(header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|_| "search API connection failed")?;
    if !response.status().is_success() {
        return Err("search API rejected the request".into());
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| "search API response failed")?;
        if bytes.len() + chunk.len() > 4 * 1024 * 1024 {
            return Err("search API response too large".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let body: Value =
        serde_json::from_slice(&bytes).map_err(|_| "search API returned invalid JSON")?;
    decode(api.config.vendor, &body)
}
fn text(value: &str, max: usize) -> String {
    let mut end = value.len().min(max);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}
pub(super) fn decode(vendor: SearchVendor, body: &Value) -> Result<Value, String> {
    let rows = match vendor {
        SearchVendor::Brave => &body["web"]["results"],
        SearchVendor::Firecrawl => body.pointer("/data/web").unwrap_or(&body["data"]),
        SearchVendor::You => body
            .pointer("/results/web")
            .or_else(|| body.get("hits"))
            .unwrap_or(&body["results"]),
        _ => &body["results"],
    };
    let mut results = Vec::new();
    for row in rows.as_array().into_iter().flatten().take(6) {
        let Some(url) = row["url"].as_str().or_else(|| row["link"].as_str()) else {
            continue;
        };
        if url.len() > 8192
            || !reqwest::Url::parse(url).is_ok_and(|u| {
                matches!(u.scheme(), "http" | "https")
                    && u.username().is_empty()
                    && u.password().is_none()
            })
        {
            continue;
        }
        let content = [
            "content",
            "text",
            "description",
            "summary",
            "snippet",
            "markdown",
        ]
        .iter()
        .find_map(|field| row[*field].as_str().filter(|s| !s.trim().is_empty()))
        .map(str::to_owned)
        .or_else(|| {
            row["snippets"].as_array().map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        })
        .unwrap_or_default();
        results.push(json!({"title":text(row["title"].as_str().unwrap_or(url),512),"url":url,"content":text(&content,8000)}));
    }
    if results.is_empty() {
        return Err("search API returned no usable results".into());
    }
    let mut out = json!({"results":results});
    while out.to_string().len() > MAX_RESULT {
        out["results"].as_array_mut().unwrap().pop();
    }
    Ok(out)
}
