//! Grok Responses compatibility. Credentials and network IO stay in the Agent.
use super::{json, ProviderStream, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
mod number;
mod schema;

#[derive(Clone, PartialEq, Eq)]
struct Name {
    name: String,
    namespace: Option<String>,
    custom: bool,
}
#[derive(Default)]
pub struct Adapter {
    names: HashMap<String, Name>,
    calls: HashMap<String, Call>,
    buffer: Vec<u8>,
    output: bool,
}
#[derive(Default)]
struct Call {
    custom: bool,
    arguments: String,
    delivered: bool,
    numbers: number::Numbers,
}
impl Adapter {
    pub fn prepare(body: &mut Value) -> Result<Self, String> {
        super::validate_request(body)?;
        if body.get("messages").is_some() {
            for tool in body
                .get_mut("tools")
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                if let Some(parameters) = tool
                    .get_mut("function")
                    .and_then(|f| f.get_mut("parameters"))
                {
                    schema::normalize(parameters);
                }
            }
            return Ok(Self::default());
        }
        let mut adapter = Self::default();
        let mut tools = Vec::new();
        for tool in body["tools"].as_array().into_iter().flatten() {
            if tool["type"] == "namespace" {
                let space = tool["name"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or("Grok namespace requires a name")?;
                let members = tool["tools"]
                    .as_array()
                    .ok_or("Grok namespace tools must be an array")?;
                for member in members {
                    let mut member = member.clone();
                    adapter.tool(&mut member, Some(space))?;
                    tools.push(member);
                }
            } else {
                let mut tool = tool.clone();
                adapter.tool(&mut tool, None)?;
                tools.push(tool);
            }
        }
        if body.get("tools").is_some() {
            body["tools"] = json!(tools);
        }
        for item in body
            .get_mut("input")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            match item["type"].as_str() {
                Some("function_call" | "custom_tool_call") => {
                    let custom = item["type"] == "custom_tool_call";
                    let name = item["name"]
                        .as_str()
                        .ok_or("Grok tool history requires a name")?
                        .to_owned();
                    let namespace = item["namespace"].as_str().map(str::to_owned);
                    let flat = adapter.register(Name {
                        name,
                        namespace,
                        custom,
                    })?;
                    item["name"] = json!(flat);
                    item.as_object_mut().unwrap().remove("namespace");
                    if custom {
                        let input = item["input"]
                            .as_str()
                            .ok_or("custom tool input must be text")?;
                        item["arguments"] = json!(json!({"input":input}).to_string());
                        item["type"] = json!("function_call");
                        item.as_object_mut().unwrap().remove("input");
                    }
                }
                Some("custom_tool_call_output") => item["type"] = json!("function_call_output"),
                Some("reasoning") if item["content"].is_null() => {
                    item.as_object_mut().unwrap().remove("content");
                }
                _ => {}
            }
        }
        if let Some(choice) = body.get_mut("tool_choice") {
            adapter.choice(choice)?;
        }
        Ok(adapter)
    }
    fn register(&mut self, name: Name) -> Result<String, String> {
        let flat = if name.namespace.is_some() || name.custom {
            let identity = json!([name.namespace, name.name, name.custom]).to_string();
            format!("aipass_grok_{:x}", Sha256::digest(identity.as_bytes()))[..44].to_owned()
        } else {
            name.name.clone()
        };
        if self.names.get(&flat).is_some_and(|old| *old != name) {
            return Err("Grok flattened tool name collision".into());
        }
        self.names.insert(flat.clone(), name);
        Ok(flat)
    }
    fn tool(&mut self, tool: &mut Value, namespace: Option<&str>) -> Result<(), String> {
        let custom = tool["type"] == "custom";
        if custom || tool["type"] == "function" {
            if custom && tool.get("format").is_some_and(|f| f["type"] != "text") {
                return Err(
                    "Grok custom tools support text format; grammar constraints are unavailable"
                        .into(),
                );
            }
            let name = tool["name"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("Grok tool requires a name")?
                .to_owned();
            let flat = self.register(Name {
                name,
                namespace: namespace.map(str::to_owned),
                custom,
            })?;
            tool["name"] = json!(flat);
            if custom {
                tool["type"] = json!("function");
                tool["parameters"] = json!({"type":"object","properties":{"input":{"type":"string"}},"required":["input"],"additionalProperties":false});
                tool.as_object_mut().unwrap().remove("format");
            }
            if let Some(parameters) = tool.get_mut("parameters") {
                schema::normalize(parameters);
            }
        } else if namespace.is_some()
            || !matches!(
                tool["type"].as_str(),
                Some(
                    "web_search"
                        | "x_search"
                        | "image_generation"
                        | "collections_search"
                        | "file_search"
                        | "code_execution"
                        | "code_interpreter"
                        | "mcp"
                        | "shell"
                        | "tool_search"
                )
            )
        {
            return Err("Grok does not support this tool type".into());
        }
        if let Some(object) = tool.as_object_mut() {
            object.remove("external_web_access");
        }
        Ok(())
    }
    fn choice(&mut self, choice: &mut Value) -> Result<(), String> {
        if choice["type"] == "allowed_tools" {
            for tool in choice["tools"].as_array_mut().into_iter().flatten() {
                self.choice(tool)?;
            }
        } else if matches!(choice["type"].as_str(), Some("function" | "custom")) {
            let name = choice["name"]
                .as_str()
                .ok_or("Grok tool choice requires a name")?
                .to_owned();
            let namespace = choice["namespace"].as_str().map(str::to_owned);
            let flat = self.register(Name {
                name,
                namespace,
                custom: choice["type"] == "custom",
            })?;
            choice["name"] = json!(flat);
            choice["type"] = json!("function");
            choice.as_object_mut().unwrap().remove("namespace");
        }
        Ok(())
    }
    fn restore_item(&self, item: &mut Value) -> Result<(), String> {
        if item["type"] != "function_call" {
            return Ok(());
        }
        let name = item["name"].as_str().and_then(|name| self.names.get(name));
        if let Some(name) = name {
            item["name"] = json!(name.name);
            if let Some(space) = &name.namespace {
                item["namespace"] = json!(space);
            }
            if name.custom {
                let args = item["arguments"].as_str().unwrap_or("");
                let input = if args.is_empty() {
                    String::new()
                } else {
                    custom_input(args)?
                };
                item["type"] = json!("custom_tool_call");
                item["input"] = json!(input);
                item.as_object_mut().unwrap().remove("arguments");
                return Ok(());
            }
        }
        if let Some(args) = item["arguments"].as_str() {
            item["arguments"] = json!(number::whole(args)?);
        }
        Ok(())
    }
    pub fn response(&self, response: &mut Value) -> Result<(), String> {
        for item in response
            .get_mut("output")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            self.restore_item(item)?;
        }
        for choice in response
            .get_mut("choices")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            for call in choice
                .get_mut("message")
                .and_then(|m| m.get_mut("tool_calls"))
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                if let Some(args) = call.pointer("/function/arguments").and_then(Value::as_str) {
                    call["function"]["arguments"] = json!(number::whole(args)?);
                }
            }
        }
        Ok(())
    }
    fn event(&mut self, mut event: Value) -> Result<Vec<Vec<u8>>, String> {
        let mut before = Vec::new();
        if let Some(choices) = event.get_mut("choices").and_then(Value::as_array_mut) {
            for choice in choices {
                let choice_index = choice["index"].as_u64().unwrap_or(0);
                for call in choice
                    .get_mut("delta")
                    .and_then(|d| d.get_mut("tool_calls"))
                    .and_then(Value::as_array_mut)
                    .into_iter()
                    .flatten()
                {
                    let index = call["index"].as_u64().unwrap_or(0);
                    let key = format!("chat:{choice_index}:{index}");
                    if let Some(args) = call.pointer("/function/arguments").and_then(Value::as_str)
                    {
                        let state = self.calls.entry(key).or_default();
                        call["function"]["arguments"] = json!(state.numbers.push(args)?);
                    }
                }
                if !choice["finish_reason"].is_null() {
                    let prefix = format!("chat:{choice_index}:");
                    for (key, call) in self
                        .calls
                        .iter_mut()
                        .filter(|(k, _)| k.starts_with(&prefix))
                    {
                        let tail = call.numbers.finish();
                        if !tail.is_empty() {
                            let index = key[prefix.len()..]
                                .parse::<u64>()
                                .map_err(|_| "invalid Grok tool index")?;
                            before.push(encode(&json!({"choices":[{"index":choice_index,"delta":{"tool_calls":[{"index":index,"function":{"arguments":tail}}]},"finish_reason":null}]})));
                        }
                    }
                }
            }
            self.output = true;
        }
        match event["type"].as_str() {
            Some("response.output_item.added" | "response.output_item.done") => {
                let item = &event["item"];
                if item["type"] == "function_call" {
                    let id = item["id"]
                        .as_str()
                        .ok_or("Grok call item requires an id")?
                        .to_owned();
                    let custom = item["name"]
                        .as_str()
                        .and_then(|n| self.names.get(n))
                        .is_some_and(|n| n.custom);
                    self.calls.entry(id.clone()).or_insert_with(|| Call {
                        custom,
                        ..Default::default()
                    });
                    if event["type"] == "response.output_item.done" && custom {
                        let call = self.calls.get_mut(&id).unwrap();
                        if !call.delivered {
                            let args = item["arguments"].as_str().unwrap_or(&call.arguments);
                            before.push(encode(&json!({"type":"response.custom_tool_call_input.delta","item_id":id,"output_index":event["output_index"],"delta":custom_input(args)?})));
                            call.delivered = true;
                        }
                    } else if event["type"] == "response.output_item.done" {
                        let tail = self.calls.get_mut(&id).unwrap().numbers.finish();
                        if !tail.is_empty() {
                            before.push(encode(&json!({"type":"response.function_call_arguments.delta","item_id":id,"output_index":event["output_index"],"delta":tail})));
                        }
                    }
                    self.restore_item(&mut event["item"])?;
                    self.output = true;
                }
            }
            Some("response.function_call_arguments.delta") => {
                let id = event["item_id"]
                    .as_str()
                    .ok_or("Grok argument delta requires an item id")?;
                let call = self.calls.entry(id.to_owned()).or_default();
                let delta = event["delta"]
                    .as_str()
                    .ok_or("Grok argument delta must be text")?;
                if call.custom {
                    call.arguments.push_str(delta);
                    if call.arguments.len() > 4 * 1024 * 1024 {
                        return Err("Grok custom arguments exceed limit".into());
                    }
                    return Ok(vec![]);
                }
                event["delta"] = json!(call.numbers.push(delta)?);
                self.output = true;
            }
            Some("response.function_call_arguments.done") => {
                let id = event["item_id"]
                    .as_str()
                    .ok_or("Grok arguments require an item id")?
                    .to_owned();
                let call = self.calls.entry(id.clone()).or_default();
                if call.custom {
                    let args = event["arguments"].as_str().unwrap_or(&call.arguments);
                    let input = custom_input(args)?;
                    if !call.delivered {
                        before.push(encode(&json!({"type":"response.custom_tool_call_input.delta","item_id":id,"output_index":event["output_index"],"delta":input})));
                        call.delivered = true;
                    }
                    event["type"] = json!("response.custom_tool_call_input.done");
                    event["input"] = json!(input);
                    event.as_object_mut().unwrap().remove("arguments");
                } else {
                    let tail = call.numbers.finish();
                    if !tail.is_empty() {
                        before.push(encode(&json!({"type":"response.function_call_arguments.delta","item_id":id,"output_index":event["output_index"],"delta":tail})));
                    }
                    if let Some(args) = event["arguments"].as_str() {
                        event["arguments"] = json!(number::whole(args)?);
                    }
                }
            }
            Some("response.output_text.delta") => self.output = true,
            _ => {}
        }
        if let Some(response) = event.get_mut("response") {
            self.response(response)?;
        }
        self.response(&mut event)?;
        before.push(encode(&event));
        Ok(before)
    }
}
fn custom_input(args: &str) -> Result<String, String> {
    let value: Value = serde_json::from_str(args).map_err(|_| "invalid Grok custom arguments")?;
    if value.as_object().is_none_or(|o| o.len() != 1) {
        return Err("invalid Grok custom arguments".into());
    }
    value["input"]
        .as_str()
        .map(str::to_owned)
        .ok_or("Grok custom input must be text".into())
}
fn encode(value: &Value) -> Vec<u8> {
    format!(
        "event: {}\ndata: {value}\n\n",
        value["type"].as_str().unwrap_or("message")
    )
    .into_bytes()
}
impl ProviderStream for Adapter {
    fn push(&mut self, bytes: &[u8]) -> Result<Vec<Vec<u8>>, String> {
        self.buffer.extend_from_slice(bytes);
        if self.buffer.len() > 4 * 1024 * 1024 {
            return Err("Grok event exceeds limit".into());
        }
        let mut out = Vec::new();
        while let Some((index, length)) = boundary(&self.buffer) {
            let bytes: Vec<_> = self.buffer.drain(..index + length).collect();
            let text = std::str::from_utf8(&bytes).map_err(|_| "invalid Grok stream encoding")?;
            let data = text
                .lines()
                .filter_map(|line| {
                    line.strip_prefix("data:")
                        .map(|s| s.strip_prefix(' ').unwrap_or(s))
                })
                .collect::<Vec<_>>()
                .join("\n");
            if data.is_empty() {
                out.push(bytes);
                continue;
            }
            if data == "[DONE]" {
                out.push(bytes);
                continue;
            }
            let event = serde_json::from_str(&data).map_err(|_| "invalid Grok stream event")?;
            out.extend(self.event(event)?);
        }
        Ok(out)
    }
    fn finish(&mut self) -> Result<Vec<Vec<u8>>, String> {
        if self.buffer.iter().any(|b| !b.is_ascii_whitespace()) {
            return Err("Grok stream ended inside an event".into());
        }
        self.buffer.clear();
        Ok(vec![])
    }
    fn has_output(&self) -> bool {
        self.output
    }
}

fn boundary(bytes: &[u8]) -> Option<(usize, usize)> {
    // SSE permits LF or CRLF for each line, including mixed pairs.
    bytes.iter().enumerate().find_map(|(i, b)| {
        if *b != b'\n' {
            return None;
        }
        match bytes.get(i + 1..) {
            Some([b'\n', ..]) => Some((i, 2)),
            Some([b'\r', b'\n', ..]) => Some((i, 3)),
            _ => None,
        }
    })
}

#[cfg(test)]
mod tests;
