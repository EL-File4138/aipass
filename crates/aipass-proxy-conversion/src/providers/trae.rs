//! Trae's chat envelope and SSE events, without credential or network access.
use super::connect::{ChatStream, Lines};
use super::ProviderStream;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
const LIMIT: usize = 8 * 1024 * 1024;

fn event_name(name: &str) -> String {
    let mut out = String::new();
    let mut lower = false;
    for c in name.trim().chars() {
        if c.is_ascii_uppercase() && lower {
            out.push('_');
        }
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
        lower = c.is_ascii_lowercase() || c.is_ascii_digit();
    }
    out.trim_end_matches('_').into()
}
fn finish_reason(value: &Value) -> &'static str {
    match value["finish_reason"]
        .as_str()
        .or(value["stop_reason"].as_str())
        .or(value["finishReason"].as_str())
    {
        Some("length" | "max_tokens" | "max_output_tokens" | "max_length") => "length",
        Some("content_filter" | "sensitive") => "content_filter",
        _ => "stop",
    }
}

mod tools;
use tools::{tool_start, ToolPolicy};

pub fn request(
    chat: &Value,
    function: &str,
    config: &str,
    native_model: &str,
    session: &str,
    images: bool,
) -> Result<Value, String> {
    if chat
        .get("response_format")
        .is_some_and(|v| !v.is_null() && v["type"] != "text")
    {
        return Err("Trae does not support structured response formats".into());
    }
    if chat
        .get("stop")
        .is_some_and(|v| !v.is_null() && v != "" && v.as_array().is_none_or(|a| !a.is_empty()))
    {
        return Err("Trae does not support stop sequences".into());
    }
    if chat
        .get("n")
        .is_some_and(|v| !v.is_null() && v.as_u64() != Some(1))
    {
        return Err("Trae supports one completion per request".into());
    }
    let policy = ToolPolicy::from_chat(chat)?;
    let mut messages = Vec::new();
    let mut calls = HashMap::new();
    let tools: Vec<_> = chat["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|t| {
            t["function"]["name"]
                .as_str()
                .is_some_and(|n| policy.names.iter().any(|allowed| allowed == n))
        })
        .collect();
    if !policy.names.is_empty() {
        let required = if policy.required {
            " You must call a listed tool."
        } else {
            ""
        };
        let parallel = if policy.parallel {
            ""
        } else {
            " Call at most one tool."
        };
        messages.push(json!({"role":"system","content":[{"type":"text","text":format!("Available tools: {}. To call a tool, emit <tool_call>{{\"name\":\"tool name\",\"arguments\":{{}}}}</tool_call> with its complete JSON arguments.{required}{parallel}",serde_json::to_string(&tools).map_err(|_|"Invalid tools")?)}]}));
    }
    for message in chat["messages"]
        .as_array()
        .ok_or("Trae messages must be an array")?
    {
        let role = message["role"].as_str().unwrap_or("");
        let mut content = message["content"].as_str().unwrap_or("").to_owned();
        let mut blocks = Vec::new();
        for part in message["content"].as_array().into_iter().flatten() {
            if part["type"] != "text" && part["type"] != "image_url" {
                return Err("Unsupported Trae content block".into());
            }
            if part["type"] == "image_url" {
                if !images {
                    return Err("This Trae model does not accept images".into());
                }
                let url = part["image_url"]["url"]
                    .as_str()
                    .ok_or("Invalid image URL")?;
                blocks.push(json!({"type":"image_url","image_url":{"url":url}}));
            } else {
                let text = part["text"].as_str().ok_or("Invalid Trae text block")?;
                blocks.push(json!({"type":"text","text":text}));
            }
        }
        if role == "assistant" {
            for call in message["tool_calls"].as_array().into_iter().flatten() {
                let id = call["id"].as_str().ok_or("Tool call ID is required")?;
                let name = call["function"]["name"]
                    .as_str()
                    .ok_or("Tool name is required")?;
                let args: Value =
                    serde_json::from_str(call["function"]["arguments"].as_str().unwrap_or("{}"))
                        .map_err(|_| "Invalid tool history")?;
                if calls.insert(id.to_owned(), name.to_owned()).is_some() {
                    return Err("Duplicate tool history ID".into());
                }
                content.push_str(&format!(
                    "\n<tool_call>{}</tool_call>",
                    json!({"id":id,"name":name,"arguments":args})
                ));
            }
        }
        let role = match role {
            "system" | "developer" => "system",
            "assistant" => "assistant",
            "user" => "user",
            "tool" => {
                let id = message["tool_call_id"]
                    .as_str()
                    .ok_or("Tool result ID is required")?;
                let name = calls.remove(id).ok_or("Tool result has no matching call")?;
                content = format!("Tool result for {name} ({id}):\n{content}");
                "user"
            }
            _ => return Err("Unsupported Trae message role".into()),
        };
        if !content.is_empty() {
            let text = json!({"type":"text","text":content});
            if role == "assistant" {
                blocks.push(text);
            } else {
                blocks.insert(0, text);
            }
        }
        if !blocks.is_empty() {
            if role != "system" && messages.last().is_some_and(|m| m["role"] == role) {
                messages.last_mut().unwrap()["content"]
                    .as_array_mut()
                    .unwrap()
                    .extend(blocks);
            } else {
                messages.push(json!({"role":role,"content":blocks}));
            }
        }
    }
    if !calls.is_empty() {
        return Err("Trae tool history has unanswered calls".into());
    }
    let mut request = json!({"messages":messages,"function":function,"config_name":config,"model":config,"stream":true,"session_id":session});
    if !native_model.is_empty() {
        request["model_name"] = json!(native_model);
    }
    for key in ["temperature", "top_p", "parallel_tool_calls"] {
        if let Some(v) = chat.get(key) {
            request[key] = v.clone();
        }
    }
    if let Some(max) = chat["max_completion_tokens"]
        .as_u64()
        .or(chat["max_tokens"].as_u64())
    {
        request["max_tokens"] = json!(max);
    }
    if !policy.names.is_empty() {
        let native:Vec<_>=tools.iter().map(|t|{let f=&t["function"];json!({"type":"function","function":{"name":f["name"],"description":f["description"].as_str().unwrap_or(""),"parameters":f["parameters"].to_string()}})}).collect();
        request["tools"] = json!(native);
        request["tool_choice"] = if chat["tool_choice"].is_object() {
            chat["tool_choice"]["function"]["name"].clone()
        } else {
            chat.get("tool_choice").cloned().unwrap_or(json!("auto"))
        };
    }
    Ok(request)
}

pub struct TraeStream {
    lines: Lines,
    chat: ChatStream,
    event: String,
    data: String,
    content: String,
    text_sent: usize,
    native: BTreeMap<u64, Value>,
    usage: Value,
    progress: bool,
    failure: Option<u16>,
    policy: ToolPolicy,
    terminal: Option<String>,
}
impl TraeStream {
    pub fn new(model: &str, id: &str, chat: &Value) -> Result<Self, String> {
        Ok(Self {
            lines: Lines::default(),
            chat: ChatStream::new(model, id),
            event: String::new(),
            data: String::new(),
            content: String::new(),
            text_sent: 0,
            native: BTreeMap::new(),
            usage: json!({}),
            progress: false,
            failure: None,
            terminal: None,
            policy: ToolPolicy::from_chat(chat)?,
        })
    }
    fn line(&mut self, line: &[u8]) -> Result<Vec<Vec<u8>>, String> {
        let line = std::str::from_utf8(line)
            .map_err(|_| "Trae stream is not UTF-8")?
            .trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            return self.flush();
        }
        if let Some(event) = line.strip_prefix("event:") {
            self.event = event_name(event);
        }
        if let Some(data) = line.strip_prefix("data:") {
            if self.data.len() + data.len() > LIMIT {
                return Err("Trae event exceeds limit".into());
            }
            self.data.push_str(data.trim_start());
            self.data.push('\n');
        }
        Ok(vec![])
    }
    fn tool(&mut self, name: &str, args: Value, id: Option<&str>) -> Result<Vec<Vec<u8>>, String> {
        if !args.is_object() || !self.policy.names.iter().any(|t| t == name) {
            return Err("Trae returned an unknown tool or invalid arguments".into());
        }
        if !self.policy.parallel && self.chat.tools > 0 {
            return Err("Trae returned parallel tools when disabled".into());
        }
        let index = self.chat.tools;
        self.chat.tools += 1;
        Ok(self.chat.delta(json!({"tool_calls":[{"index":index,"id":id.map(str::to_owned).unwrap_or_else(||format!("{}_tool_{index}",self.chat.id)),"type":"function","function":{"name":name,"arguments":args.to_string()}}]})))
    }
    fn finish_text(&mut self) -> Result<Vec<Vec<u8>>, String> {
        let content = std::mem::take(&mut self.content);
        let mut tail = content.as_str();
        let sent = std::mem::take(&mut self.text_sent);
        let mut out = vec![];
        if self.policy.names.is_empty() {
            if !self.native.is_empty() {
                return Err("Trae returned tools when disabled".into());
            }
            return Ok(if content.is_empty() {
                out
            } else {
                self.chat.delta(json!({"content":content}))
            });
        }
        if !self.native.is_empty() && tool_start(tail).is_some() {
            return Err("Trae returned duplicate tool representations".into());
        }
        while let Some(start) = tool_start(tail) {
            let offset = content.len() - tail.len();
            let skip = sent.saturating_sub(offset).min(start);
            if start > skip {
                out.extend(self.chat.delta(json!({"content":&tail[skip..start]})));
            }
            let (name, args, remaining) = self.policy.parse(&tail[start..])?;
            out.extend(self.tool(&name, args, None)?);
            tail = remaining;
        }
        let skip = sent
            .saturating_sub(content.len() - tail.len())
            .min(tail.len());
        if skip < tail.len() {
            out.extend(self.chat.delta(json!({"content":&tail[skip..]})));
        }
        let mut ids = std::collections::HashSet::new();
        for (_, call) in std::mem::take(&mut self.native) {
            if let Some(id) = call["id"].as_str().filter(|s| !s.is_empty()) {
                if !ids.insert(id.to_owned()) {
                    return Err("Trae returned duplicate tool IDs".into());
                }
            }
            let args: Value =
                tools::strict_json(call["function"]["arguments"].as_str().unwrap_or("{}"))?;
            let name = call["function"]["name"]
                .as_str()
                .ok_or("Trae returned no native tool name")?;
            if name.starts_with("<tool_call") {
                let (name, embedded, tail) = self.policy.parse(name)?;
                if !tail.trim().is_empty() || args != json!({}) {
                    return Err("Trae returned duplicate native tool arguments".into());
                }
                out.extend(self.tool(&name, embedded, call["id"].as_str())?);
            } else {
                out.extend(self.tool(name, args, call["id"].as_str())?);
            }
        }
        if self.policy.required && self.chat.tools == 0 {
            return Err("Trae did not return the required tool call".into());
        }
        Ok(out)
    }
    fn flush(&mut self) -> Result<Vec<Vec<u8>>, String> {
        if self.data.is_empty() {
            return Ok(vec![]);
        }
        let raw = std::mem::take(&mut self.data);
        let event = std::mem::take(&mut self.event);
        if raw.trim() == "[DONE]" {
            if self.chat.done {
                return Ok(vec![]);
            }
            let mut out = self.finish_text()?;
            let usage = self.final_usage();
            out.extend(
                self.chat
                    .stop(self.terminal.as_deref().unwrap_or("stop"), usage)?,
            );
            return Ok(out);
        }
        let v: Value =
            serde_json::from_str(&raw).map_err(|_| "Trae returned invalid event JSON")?;
        if self.chat.done {
            return Err("Trae sent output after completion".into());
        }
        let event = if event.is_empty() {
            event_name(
                v["event"]
                    .as_str()
                    .or(v["type"].as_str())
                    .or(v["name"].as_str())
                    .unwrap_or(""),
            )
        } else {
            event
        };
        if event == "error"
            || (event != "output"
                && v["code"].as_i64().is_some_and(|n| n != 0)
                && v["message"].is_string())
        {
            let message = v["message"].as_str().unwrap_or("Trae refused generation");
            self.failure = Some(match v["code"].as_i64() {
                Some(1001 | 20101 | 401) => 401,
                Some(4008 | 1005 | 4011) => 429,
                _ => super::failure_status(message),
            });
            return Err(message.chars().take(300).collect());
        }
        let usage = if event == "token_usage" || event == "tokenusage" {
            v.get("usage").filter(|u| u.is_object()).unwrap_or(&v)
        } else {
            &v["usage"]
        };
        for (out, keys) in [
            (
                "prompt_tokens",
                &[
                    "prompt_tokens",
                    "input_tokens",
                    "promptTokens",
                    "inputTokens",
                ][..],
            ),
            (
                "completion_tokens",
                &[
                    "completion_tokens",
                    "output_tokens",
                    "completionTokens",
                    "outputTokens",
                ][..],
            ),
            ("total_tokens", &["total_tokens", "totalTokens"][..]),
        ] {
            if let Some(n) = keys.iter().find_map(|key| usage[*key].as_u64()) {
                self.usage[out] = json!(n);
            }
        }
        for field in ["cache_read_input_tokens", "cache_creation_input_tokens"] {
            if let Some(n) = usage[field].as_u64() {
                self.usage[field] = json!(n);
            }
        }
        for field in ["prompt_tokens_details", "completion_tokens_details"] {
            if let Some(details) = usage[field].as_object() {
                for (key, value) in details {
                    if let Some(n) = value.as_u64() {
                        self.usage[field][key] = json!(n);
                    }
                }
            }
        }
        if let Some(n) = usage["reasoning_tokens"].as_u64() {
            self.usage["completion_tokens_details"]["reasoning_tokens"] = json!(n);
        }
        if matches!(event.as_str(), "done" | "response_done" | "stream_done") {
            self.terminal = Some(finish_reason(&v).into());
            return Ok(vec![]);
        }
        if matches!(event.as_str(), "token_usage" | "tokenusage") {
            return Ok(vec![]);
        }
        if self.terminal.is_some() {
            return Err("Trae sent content after its terminal event".into());
        }
        let mut out = vec![];
        if let Some(reasoning) = v["reasoning_content"]
            .as_str()
            .or(v["reasoning"].as_str())
            .or(v["delta"]["reasoning_content"].as_str())
            .filter(|s| !s.is_empty())
        {
            self.progress = true;
            out.extend(self.chat.delta(json!({"reasoning_content":reasoning})));
        }
        if let Some(text) = v["response"]
            .as_str()
            .or(v["content"].as_str())
            .or(v["delta"]["content"].as_str())
            .or(v["delta"].as_str())
            .filter(|s| !s.is_empty())
        {
            self.progress = true;
            if self.policy.names.is_empty() {
                out.extend(self.chat.delta(json!({"content":text})));
            } else {
                if self.content.len() + text.len() > LIMIT {
                    return Err("Trae text exceeds limit".into());
                }
                self.content.push_str(text);
                let safe = tool_start(&self.content).unwrap_or_else(|| {
                    let marker = "<tool_call";
                    let pending = (1..marker.len())
                        .rev()
                        .find(|n| self.content.ends_with(&marker[..*n]))
                        .unwrap_or(0);
                    self.content.len() - pending
                });
                if safe > self.text_sent {
                    out.extend(
                        self.chat
                            .delta(json!({"content":&self.content[self.text_sent..safe]})),
                    );
                    self.text_sent = safe;
                }
            }
        }
        for (position, call) in v["tool_calls"]
            .as_array()
            .or(v["delta"]["tool_calls"].as_array())
            .into_iter()
            .flatten()
            .enumerate()
        {
            self.progress = true;
            let index = call["index"].as_u64().unwrap_or(position as u64);
            if index > 65535 {
                return Err("Trae tool index exceeds limit".into());
            }
            let entry = self
                .native
                .entry(index)
                .or_insert_with(|| json!({"function":{"name":"","arguments":""}}));
            if let Some(id) = call["id"].as_str() {
                entry["id"] = json!(id);
            }
            for key in ["name", "arguments"] {
                if call["function"].get(key).is_some_and(|v| !v.is_string()) {
                    return Err("Trae returned invalid native tool fields".into());
                }
                if let Some(delta) = call["function"][key].as_str() {
                    let old = entry["function"][key].as_str().unwrap_or("");
                    if old.len() + delta.len() > LIMIT {
                        return Err("Trae tool exceeds limit".into());
                    }
                    entry["function"][key] = json!(format!("{old}{delta}"));
                }
            }
        }
        Ok(out)
    }
    fn final_usage(&self) -> Option<Value> {
        if self.usage.as_object()?.is_empty() {
            return None;
        }
        let mut usage = self.usage.clone();
        if usage["total_tokens"].is_null() {
            if let (Some(input), Some(output)) = (
                usage["prompt_tokens"].as_u64(),
                usage["completion_tokens"].as_u64(),
            ) {
                if let Some(total) = input.checked_add(output) {
                    usage["total_tokens"] = json!(total);
                }
            }
        }
        Some(usage)
    }
}
impl ProviderStream for TraeStream {
    fn push(&mut self, bytes: &[u8]) -> Result<Vec<Vec<u8>>, String> {
        let mut out = vec![];
        for line in self.lines.push(bytes)? {
            out.extend(self.line(&line)?);
        }
        Ok(out)
    }
    fn finish(&mut self) -> Result<Vec<Vec<u8>>, String> {
        let mut out = vec![];
        for line in self.lines.finish() {
            out.extend(self.line(&line)?);
        }
        out.extend(self.flush()?);
        if !self.chat.done {
            if let Some(reason) = self.terminal.take() {
                out.extend(self.finish_text()?);
                let usage = self.final_usage();
                out.extend(self.chat.stop(&reason, usage)?);
            }
        }
        self.chat.finish()?;
        Ok(out)
    }
    fn has_output(&self) -> bool {
        self.progress
    }
    fn failure_status(&self) -> Option<u16> {
        self.failure
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tools() -> Value {
        json!([{"type":"function","function":{"name":"lookup","parameters":{"type":"object"}}}])
    }
    #[test]
    fn plain_text_arrives_before_completion_without_duplicate_content() {
        for chat in [json!({}), json!({"tools":tools(),"tool_choice":"none"})] {
            let mut stream = TraeStream::new("m", "id", &chat).unwrap();
            let first = stream
                .push(b"event: output\ndata: {\"response\":\"first chunk\"}\n\n")
                .unwrap();
            let first = String::from_utf8(first.concat()).unwrap();
            assert!(first.contains("first chunk"));
            assert!(!first.contains("[DONE]"));
            let next = stream.push(b"event: output\ndata: {\"response\":\"second chunk\"}\n\nevent: done\ndata: {}\n\n").unwrap();
            let final_chunks = stream.finish().unwrap();
            let rest = String::from_utf8([next.concat(), final_chunks.concat()].concat()).unwrap();
            assert!(rest.contains("second chunk"));
            assert!(!rest.contains("first chunk"));
            assert_eq!(rest.matches("data: [DONE]").count(), 1);
        }
    }
    #[test]
    fn unsupported_output_constraints_fail_before_dispatch() {
        let mut chat = json!({"messages":[{"role":"user","content":"reply"}]});
        for (field, value) in [
            (
                "response_format",
                json!({"type":"json_schema","json_schema":{"name":"answer","schema":{"type":"object"}}}),
            ),
            ("response_format", json!({"type":"json_object"})),
            ("stop", json!(["END"])),
            ("stop", json!("END")),
            ("n", json!(2)),
        ] {
            chat[field] = value;
            assert!(request(&chat, "chat_v3", "m", "", "session", false).is_err());
            chat.as_object_mut().unwrap().remove(field);
        }
        chat["response_format"] = json!({"type":"text"});
        chat["stop"] = json!([]);
        chat["n"] = json!(1);
        assert!(request(&chat, "chat_v3", "m", "", "session", false).is_ok());
    }
    fn decode(chat: &Value, text: &str) -> Result<String, String> {
        let mut decoder = TraeStream::new("m", "id", chat)?;
        let events = format!(
            "event: output\ndata: {}\n\nevent: done\ndata: {{}}\n\n",
            json!({"response":text})
        );
        let mut chunks = decoder.push(events.as_bytes())?;
        chunks.extend(decoder.finish()?);
        String::from_utf8(chunks.concat()).map_err(|_| "Invalid test UTF-8".into())
    }
    #[test]
    fn glm_attributes_and_native_name_envelopes_keep_declared_parameter_types() {
        let chat = json!({"tools":[{"type":"function","function":{"name":"lookup","parameters":{"type":"object","properties":{"text":{"type":"string"},"n":{"type":"integer"}}}}}]});
        for call in [
            "<tool_call>lookup<arg_key>text</arg_key><arg_value>123</arg_value><arg_key>n</arg_key><arg_value>3</arg_value></tool_call>",
            "<tool_call name=\"lookup\"><parameter name=\"text\">123</parameter><parameter name=\"n\">3</parameter></tool_call>",
        ] {
            let output = decode(&chat,call).unwrap();
            assert!(output.contains(r#"\"text\":\"123\""#));
            assert!(output.contains(r#"\"n\":3"#));
            let mut stream = TraeStream::new("m","id",&chat).unwrap();
            let event = format!("data: {}\n\ndata: [DONE]\n\n",json!({"delta":{"tool_calls":[{"index":0,"id":"one","function":{"name":call,"arguments":"{}"}}]}}));
            let mut chunks=Vec::new();
            for byte in event.as_bytes().chunks(1) { chunks.extend(stream.push(byte).unwrap()); }
            chunks.extend(stream.finish().unwrap());
            assert!(String::from_utf8(chunks.concat()).unwrap().contains(r#""finish_reason":"tool_calls""#));
        }
        for invalid in [
            "<tool_call>lookup<arg_key>n</arg_key><arg_value>3.5</arg_value></tool_call>",
            "<tool_call>lookup<arg_key>text</arg_key><arg_value>x</arg_value><arg_key>text</arg_key><arg_value>y</arg_value></tool_call>",
            "<tool_call>lookup<arg_key>text</arg_key><arg_value>truncated",
            r#"<tool_call>{"name":"lookup","arguments":{"n":1,"n":2}}</tool_call>"#,
            r#"<tool_call>{"name":"lookup","name":"unknown","arguments":{}}</tool_call>"#,
        ] { assert!(decode(&chat,invalid).is_err(),"{invalid}"); }
    }
    #[test]
    fn tool_enabled_prose_streams_before_terminal_without_leaking_split_markup() {
        let chat = json!({"tools":tools()});
        let mut stream = TraeStream::new("m", "id", &chat).unwrap();
        let mut all = Vec::new();
        for (i, text) in [
            "正在查找",
            "<tool_",
            "call>{\"name\":\"lookup\",\"arguments\":{}}</tool_call>",
        ]
        .iter()
        .enumerate()
        {
            let event = format!("event: output\ndata: {}\n\n", json!({"response":text}));
            let chunks = stream.push(event.as_bytes()).unwrap();
            if i == 0 {
                assert!(String::from_utf8(chunks.concat())
                    .unwrap()
                    .contains("正在查找"));
            }
            if i == 1 {
                assert!(chunks.is_empty());
            }
            all.extend(chunks);
        }
        all.extend(stream.push(b"data: [DONE]\n\n").unwrap());
        all.extend(stream.finish().unwrap());
        let out = String::from_utf8(all.concat()).unwrap();
        assert_eq!(out.matches("正在查找").count(), 1);
        assert!(!out.contains("<tool_"));
        assert!(out.contains("tool_calls"));
    }
    #[test]
    fn tool_choice_and_parallel_limits_are_enforced_on_returned_tools() {
        let call = r#"<tool_call>{"name":"lookup","arguments":{}}</tool_call>"#;
        let disabled = decode(&json!({"tools":tools(),"tool_choice":"none"}), call).unwrap();
        assert!(disabled.contains("<tool_call>"));
        assert!(!disabled.contains(r#""finish_reason":"tool_calls""#));
        assert!(decode(
            &json!({"tools":tools(),"tool_choice":"required"}),
            "plain answer"
        )
        .is_err());
        assert!(
            decode(&json!({"tools":tools(),"tool_choice":"required"}), call)
                .unwrap()
                .contains(r#""finish_reason":"tool_calls""#)
        );
        assert!(decode(
            &json!({"tools":tools(),"parallel_tool_calls":false}),
            &format!("{call}{call}")
        )
        .is_err());
        let both = json!([{"type":"function","function":{"name":"lookup"}},{"type":"function","function":{"name":"other"}}]);
        let named =
            json!({"tools":both,"tool_choice":{"type":"function","function":{"name":"other"}}});
        assert!(decode(&named, call).is_err());
        let mut named_request = named.clone();
        named_request["messages"] = json!([{"role":"user","content":"use it"}]);
        let mapped = request(&named_request, "chat_v3", "m", "", "session", false).unwrap();
        assert_eq!(mapped["tools"].as_array().unwrap().len(), 1);
        assert_eq!(mapped["tools"][0]["function"]["name"], "other");
        assert!(TraeStream::new("m", "id", &json!({"tools":tools(),"tool_choice":{"type":"function","function":{"name":"missing"}}})).is_err());
        let mut native =
            TraeStream::new("m", "id", &json!({"tools":tools(),"tool_choice":"none"})).unwrap();
        native.push(b"data: {\"delta\":{\"tool_calls\":[{\"function\":{\"name\":\"lookup\",\"arguments\":\"{}\"}}]}}\n\nevent: done\ndata: {}\n\n").unwrap();
        assert!(native.finish().is_err());
        assert!(native.has_output());
    }
    #[test]
    fn quoted_examples_are_text_and_tool_arguments_may_contain_closing_tags() {
        let call =
            r#"<tool_call>{"name":"lookup","arguments":{"text":"</tool_call>"}}</tool_call>"#;
        for quoted in [
            format!("```json\n{call}\n```"),
            format!("`{call}`"),
            format!("> {call}"),
            format!("  > {call}"),
            format!("~~~\n{call}\n~~~"),
        ] {
            let output = decode(&json!({"tools":tools()}), &quoted).unwrap();
            assert!(!output.contains(r#""finish_reason":"tool_calls""#));
            assert!(output.contains("<tool_call>"));
        }
        let output = decode(&json!({"tools":tools()}), call).unwrap();
        assert!(output.contains(r#""finish_reason":"tool_calls""#));
        assert!(output.contains("</tool_call>"));
    }
    #[test]
    fn nested_usage_and_camel_case_terminals_keep_counts_and_stop_reason() {
        let events = b"event: Output\ndata: {\"response\":\"ok\"}\n\nevent: TokenUsage\ndata: {\"usage\":{\"inputTokens\":8,\"outputTokens\":2,\"reasoning_tokens\":0}}\n\nevent: ResponseDone\ndata: {\"stop_reason\":\"max_output_tokens\"}\n\n";
        let mut decoder = TraeStream::new("m", "id", &json!({})).unwrap();
        let mut chunks = decoder.push(events).unwrap();
        chunks.extend(decoder.finish().unwrap());
        let output = String::from_utf8(chunks.concat()).unwrap();
        assert!(output.contains(r#""prompt_tokens":8"#));
        assert!(output.contains(r#""total_tokens":10"#));
        assert!(output.contains(r#""reasoning_tokens":0"#));
        assert!(output.contains(r#""finish_reason":"length""#));
        assert_eq!(
            finish_reason(&json!({"finishReason":"sensitive"})),
            "content_filter"
        );
    }
    #[test]
    fn split_utf8_text_tools_and_usage_form_a_complete_chat_stream() {
        let events=format!("event: output\ndata: {}\n\nevent: token_usage\ndata: {}\n\nevent: done\ndata: {{}}\n\n",json!({"response":"你好<tool_call>{\"name\":\"lookup\",\"arguments\":{\"n\":1}}</tool_call>"}),json!({"prompt_tokens":5,"completion_tokens":0}));
        for size in [1, 7, events.len()] {
            let mut stream = TraeStream::new("model", "chat-1", &json!({"tools":tools()})).unwrap();
            let mut out = vec![];
            for chunk in events.as_bytes().chunks(size) {
                out.extend(stream.push(chunk).unwrap());
            }
            out.extend(stream.finish().unwrap());
            let text = String::from_utf8(out.concat()).unwrap();
            assert!(text.contains("你好"));
            assert!(text.contains("tool_calls"));
            assert!(text.contains("lookup"));
            assert!(text.contains("\"completion_tokens\":0"));
            assert!(text.ends_with("data: [DONE]\n\n"));
        }
    }
    #[test]
    fn image_and_text_order_is_preserved_and_unsupported_blocks_fail_before_dispatch() {
        let content = json!([{"type":"text","text":"first"},{"type":"image_url","image_url":{"url":"data:image/png;base64,AAAA"}},{"type":"text","text":"second"}]);
        let body = json!({"messages":[{"role":"user","content":content}]});
        let mapped = request(&body, "chat_v3", "model", "", "session", true).unwrap();
        assert_eq!(
            mapped["messages"][0]["content"],
            body["messages"][0]["content"]
        );
        assert!(request(&body, "chat_v3", "model", "", "session", false).is_err());
        let mut audio = body;
        audio["messages"][0]["content"] =
            json!([{"type":"input_audio","input_audio":{"data":"AAAA"}}]);
        assert!(request(&audio, "chat_v3", "model", "", "session", true).is_err());
    }
    #[test]
    fn partial_generation_and_incomplete_tools_cannot_be_replayed() {
        let mut stream = TraeStream::new("m", "id", &json!({"tools":tools()})).unwrap();
        assert!(stream
            .push(b"event: output\ndata: {\"response\":\"<tool_call>{\"}\n\n")
            .is_ok());
        assert!(stream.push(b"event: done\ndata: {}\n\n").is_ok());
        assert!(stream.finish().is_err());
        assert!(stream.has_output());
        assert_eq!(stream.failure_status(), None);
        let mut refused = TraeStream::new("m", "id", &json!({"tools":tools()})).unwrap();
        assert!(refused
            .push(b"event: error\ndata: {\"code\":4008,\"message\":\"quota exhausted\"}\n\n")
            .is_err());
        assert!(!refused.has_output());
        assert_eq!(refused.failure_status(), Some(429));
    }
    #[test]
    fn trailing_usage_after_done_is_counted_once_and_the_done_sentinel_is_accepted() {
        let events=b"event: output\ndata: {\"response\":\"ok\"}\n\nevent: done\ndata: {}\n\nevent: token_usage\ndata: {\"prompt_tokens\":8,\"completion_tokens\":0,\"cache_read_input_tokens\":3}\n\ndata: [DONE]\n\n";
        let mut decoder = TraeStream::new("m", "id", &json!({})).unwrap();
        let mut chunks = decoder.push(events).unwrap();
        chunks.extend(decoder.finish().unwrap());
        let output = String::from_utf8(chunks.concat()).unwrap();
        assert!(output.contains("\"cache_read_input_tokens\":3"));
        assert_eq!(output.matches("data: [DONE]").count(), 1);
    }
    #[test]
    fn tool_result_batches_keep_call_pairing_and_merge_adjacent_user_turns() {
        let body = json!({"model":"m","messages":[{"role":"user","content":"find it"},{"role":"assistant","tool_calls":[{"id":"a","function":{"name":"lookup","arguments":"{\"n\":1}"}},{"id":"b","function":{"name":"lookup","arguments":"{\"n\":2}"}}]},{"role":"tool","tool_call_id":"a","content":"first"},{"role":"tool","tool_call_id":"b","content":"second"}],"tools":tools()});
        let request = request(&body, "solo_agent", "m", "m__dev", "session-a", false).unwrap();
        let messages = request["messages"].as_array().unwrap();
        assert_eq!(messages.last().unwrap()["role"], "user");
        assert_eq!(
            messages.last().unwrap()["content"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(request["function"], "solo_agent");
        assert_eq!(request["session_id"], "session-a");
        let assistant = messages.iter().find(|m| m["role"] == "assistant").unwrap();
        assert!(assistant["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains(r#""id":"a""#));
        assert!(assistant["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains(r#""id":"b""#));
        let mut unknown = body;
        unknown["messages"][2]["tool_call_id"] = json!("foreign");
        assert!(super::request(&unknown, "solo_agent", "m", "", "session-a", false).is_err());
    }
}
