//! The hidden CLI may search, but must never act on the local workspace.
use super::*;
pub(super) fn search_requested(payload: &Value) -> Result<bool, String> {
    let mut requested = false;
    for tool in payload["tools"].as_array().into_iter().flatten() {
        let kind = tool["type"].as_str().unwrap_or("");
        if kind.starts_with("web_search_") {
            if tool["name"] != "web_search" {
                return Err("native web search must be named web_search".into());
            }
            if tool.as_object().is_none_or(|o| {
                o.keys()
                    .any(|k| !matches!(k.as_str(), "type" | "name" | "cache_control"))
            }) {
                return Err("Claude CLI native search cannot enforce tool-level search options; use a search API route for constrained searches".into());
            }
            requested = true;
        } else if tool.get("type").is_some_and(|v| v != "custom") {
            return Err(
                "Claude subscription supports caller function tools and native web search only"
                    .into(),
            );
        }
    }
    Ok(requested)
}
pub(super) fn configure(command: &mut Command, web: bool) {
    command
        .arg("--tools")
        .arg(if web { "WebSearch" } else { "" })
        .arg("--allowedTools")
        .arg(if web {
            "mcp__aipass__*,WebSearch"
        } else {
            "mcp__aipass__*"
        });
}
/// Claude Code executes its WebSearch tool internally. Merge its assistant
/// messages into one public reply, hiding only that tool's client-side call.
#[derive(Default)]
pub(super) struct Events {
    started: bool,
    next: u64,
    indices: std::collections::HashMap<u64, u64>,
    own: bool,
    caller: bool,
    current: Value,
    before: Value,
    searches: u64,
}
impl Events {
    pub(super) fn adapt(&mut self, mut event: Value) -> Result<Option<Value>, String> {
        match event["type"].as_str() {
            Some("message_start") => {
                self.indices.clear();
                self.own = false;
                self.caller = false;
                self.current = event["message"]["usage"].clone();
                if self.started {
                    return Ok(None);
                }
                self.started = true;
            }
            Some("content_block_start") => {
                let old = event["index"]
                    .as_u64()
                    .ok_or("invalid Claude block index")?;
                let block = &event["content_block"];
                if block["type"] == "tool_use" && block["name"] == "WebSearch" {
                    self.own = true;
                    self.searches += 1;
                    return Ok(None);
                }
                if block["type"] == "tool_use" {
                    self.caller = true;
                }
                self.indices.insert(old, self.next);
                event["index"] = json!(self.next);
                self.next += 1;
            }
            Some("content_block_delta" | "content_block_stop") => {
                let old = event["index"]
                    .as_u64()
                    .ok_or("invalid Claude block index")?;
                let Some(index) = self.indices.get(&old) else {
                    return Ok(None);
                };
                event["index"] = json!(index);
            }
            Some("message_delta") => {
                if let Some(usage) = event["usage"].as_object() {
                    for (k, v) in usage {
                        self.current[k] = v.clone();
                    }
                }
                if self.own && !self.caller {
                    return Ok(None);
                }
                event["usage"] = sum(&self.before, &self.current);
                if self.searches > 0 {
                    event["usage"]["server_tool_use"] =
                        json!({"web_search_requests":self.searches});
                }
            }
            Some("message_stop") => {
                if self.own && !self.caller {
                    self.before = sum(&self.before, &self.current);
                    return Ok(None);
                }
                *self = Self::default();
            }
            _ => {}
        }
        Ok(Some(event))
    }
}
fn sum(a: &Value, b: &Value) -> Value {
    let mut out = a.as_object().cloned().unwrap_or_default();
    for (k, v) in b.as_object().into_iter().flatten() {
        if let Some(n) = v.as_u64() {
            out.insert(
                k.clone(),
                json!(out
                    .get(k)
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    .saturating_add(n)),
            );
        } else if v.is_object() {
            let value = sum(out.get(k).unwrap_or(&Value::Null), v);
            out.insert(k.clone(), value);
        } else {
            out.insert(k.clone(), v.clone());
        }
    }
    Value::Object(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_search_is_allowed_on_explicit_request() {
        assert!(
            !search_requested(&json!({"tools":[{"name":"WebSearch","input_schema":{}}]})).unwrap()
        );
        assert!(search_requested(
            &json!({"tools":[{"type":"web_search_20250305","name":"web_search"}]})
        )
        .unwrap());
        assert!(search_requested(
            &json!({"tools":[{"type":"computer_20250124","name":"computer"}]})
        )
        .is_err());
        for web in [false, true] {
            let mut c = Command::new("claude");
            configure(&mut c, web);
            let args = c
                .get_args()
                .map(|v| v.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            assert_eq!(args[1], if web { "WebSearch" } else { "" });
            assert!(!args
                .iter()
                .any(|v| v.contains("Bash") || v.contains("Read") || v.contains("Write")));
        }
    }
    #[test]
    fn search_rounds_are_one_reply_with_aggregated_usage() {
        let mut events = Events::default();
        let mut output = Vec::new();
        for e in [
            json!({"type":"message_start","message":{"usage":{"input_tokens":10}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","name":"WebSearch"}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":3}}),
            json!({"type":"message_stop"}),
            json!({"type":"message_start","message":{"usage":{"input_tokens":20}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":"answer"}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":4}}),
            json!({"type":"message_stop"}),
        ] {
            if let Some(e) = events.adapt(e).unwrap() {
                output.push(e);
            }
        }
        assert_eq!(
            output
                .iter()
                .filter(|e| e["type"] == "message_start")
                .count(),
            1
        );
        assert_eq!(
            output
                .iter()
                .filter(|e| e["type"] == "message_stop")
                .count(),
            1
        );
        let usage = &output
            .iter()
            .find(|e| e["type"] == "message_delta")
            .unwrap()["usage"];
        assert_eq!(usage["input_tokens"], 30);
        assert_eq!(usage["output_tokens"], 7);
        assert_eq!(usage["server_tool_use"]["web_search_requests"], 1);
        assert!(!output
            .iter()
            .any(|e| e["content_block"]["name"] == "WebSearch"));
    }
}
