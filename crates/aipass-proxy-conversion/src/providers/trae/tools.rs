use serde_json::{json, Value};
use std::collections::HashMap;
// Tool examples in Markdown are prose. Only unquoted markers can dispatch a
// text tool; the entire response is available here, including split delimiters.
pub(super) fn tool_start(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let (mut i, mut inline, mut fence, mut line_start) = (0, 0, None, true);
    while i < bytes.len() {
        let c = bytes[i];
        if line_start && c == b'>' {
            i += bytes[i..]
                .iter()
                .position(|b| *b == b'\n')
                .unwrap_or(bytes.len() - i);
            line_start = false;
            continue;
        }
        if c == b'`' || c == b'~' {
            let count = bytes[i..].iter().take_while(|b| **b == c).count();
            if let Some((mark, width)) = fence {
                if line_start && c == mark && count >= width {
                    fence = None;
                }
            } else if inline == 0 && line_start && count >= 3 {
                fence = Some((c, count));
            } else if c == b'`' {
                if inline == 0 {
                    inline = count;
                } else if inline == count {
                    inline = 0;
                }
            }
            i += count;
            line_start = false;
            continue;
        }
        if fence.is_none() && inline == 0 && bytes[i..].starts_with(b"<tool_call") {
            return Some(i);
        }
        if c == b'\n' {
            line_start = true;
        } else if !c.is_ascii_whitespace() {
            line_start = false;
        }
        i += 1;
    }
    None
}

pub(super) struct ToolPolicy {
    pub names: Vec<String>,
    schemas: HashMap<String, Value>,
    pub required: bool,
    pub parallel: bool,
}
impl ToolPolicy {
    pub fn from_chat(chat: &Value) -> Result<Self, String> {
        let tools = match chat.get("tools") {
            Some(v) => v
                .as_array()
                .ok_or("Trae tools must be an array")?
                .as_slice(),
            None => &[],
        };
        let mut names = Vec::new();
        for tool in tools {
            let name = tool["function"]["name"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("Trae tool name is required")?;
            if tool["type"] != "function" || names.iter().any(|n| n == name) {
                return Err("Trae requires unique function tools".into());
            }
            names.push(name.to_owned());
        }
        let choice = &chat["tool_choice"];
        let required = match choice.as_str() {
            Some("none") => {
                names.clear();
                false
            }
            Some("auto") => false,
            Some("required") => true,
            None if choice.is_null() => false,
            None if choice["type"] == "function" => {
                let name = choice["function"]["name"]
                    .as_str()
                    .ok_or("Invalid Trae tool choice")?;
                if !names.iter().any(|n| n == name) {
                    return Err("Trae tool choice is not declared".into());
                }
                names.retain(|n| n == name);
                true
            }
            _ => return Err("Unsupported Trae tool choice".into()),
        };
        if required && names.is_empty() {
            return Err("Trae tool choice requires tools".into());
        }
        let schemas = tools
            .iter()
            .map(|t| {
                (
                    t["function"]["name"].as_str().unwrap().to_owned(),
                    t["function"]["parameters"].clone(),
                )
            })
            .collect();
        Ok(Self {
            names,
            schemas,
            required,
            parallel: chat["parallel_tool_calls"] != false,
        })
    }
}

impl ToolPolicy {
    fn argument(&self, tool: &str, key: &str, raw: &str) -> Result<Value, String> {
        let schema = &self.schemas[tool];
        let field = &schema["properties"][key];
        if field.is_null() && schema["additionalProperties"] == false {
            return Err("Trae returned an undeclared parameter".into());
        }
        // GLM strings are literal, even when their contents look like JSON.
        if field["type"] == "string" {
            return Ok(json!(raw));
        }
        let value = strict_json(raw)?;
        let valid = match field["type"].as_str() {
            Some("integer") => value.is_i64() || value.is_u64(),
            Some("number") => value.is_number(),
            Some("boolean") => value.is_boolean(),
            Some("object") => value.is_object(),
            Some("array") => value.is_array(),
            Some("null") => value.is_null(),
            None => true,
            _ => false,
        };
        if !valid {
            return Err("Trae parameter type does not match its schema".into());
        }
        Ok(value)
    }
    pub fn parse<'a>(&self, text: &'a str) -> Result<(String, Value, &'a str), String> {
        let (name, args, tail) = if let Some(rest) = text.strip_prefix("<tool_call>") {
            let rest = rest.trim_start();
            if rest.starts_with('{') {
                // Decode one complete JSON value before looking for the closing
                // marker, which may legitimately occur inside an argument string.
                let mut stream = serde_json::Deserializer::from_str(rest).into_iter::<Strict>();
                let call = stream
                    .next()
                    .ok_or("Missing Trae tool JSON")?
                    .map_err(|_| "Invalid Trae tool JSON")?
                    .0;
                let after = &rest[stream.byte_offset()..];
                let tail = after
                    .trim_start()
                    .strip_prefix("</tool_call>")
                    .ok_or("Incomplete Trae tool call")?;
                let args = if let Some(raw) = call["arguments"].as_str() {
                    strict_json(raw)?
                } else {
                    call["arguments"].clone()
                };
                (
                    call["name"]
                        .as_str()
                        .ok_or("Missing Trae tool name")?
                        .to_owned(),
                    args,
                    tail,
                )
            } else {
                let (name, mut rest) = rest
                    .split_once("<arg_key>")
                    .ok_or("Invalid Trae GLM tool call")?;
                let name = name.trim();
                if !self.names.iter().any(|n| n == name) {
                    return Err("Unknown Trae tool".into());
                }
                let mut args = serde_json::Map::new();
                loop {
                    let (key, after) = rest
                        .split_once("</arg_key>")
                        .ok_or("Incomplete Trae argument key")?;
                    let key = key.trim();
                    let after = after
                        .trim_start()
                        .strip_prefix("<arg_value>")
                        .ok_or("Missing Trae argument value")?;
                    let (raw, after) = after
                        .split_once("</arg_value>")
                        .ok_or("Incomplete Trae argument value")?;
                    if key.is_empty() || args.contains_key(key) {
                        return Err("Duplicate or empty Trae parameter".into());
                    }
                    args.insert(key.to_owned(), self.argument(name, key, raw)?);
                    let after = after.trim_start();
                    if let Some(tail) = after.strip_prefix("</tool_call>") {
                        break (name.to_owned(), Value::Object(args), tail);
                    }
                    rest = after
                        .strip_prefix("<arg_key>")
                        .ok_or("Incomplete Trae GLM tool call")?;
                }
            }
        } else if let Some(rest) = text.strip_prefix("<tool_call name=\"") {
            let (name, after) = rest
                .split_once("\">")
                .ok_or("Invalid Trae tool attribute")?;
            if !self.names.iter().any(|n| n == name) {
                return Err("Unknown Trae tool".into());
            }
            let mut rest = after.trim_start();
            let mut args = serde_json::Map::new();
            loop {
                if let Some(tail) = rest.strip_prefix("</tool_call>") {
                    break (name.to_owned(), Value::Object(args), tail);
                }
                let after = rest
                    .strip_prefix("<parameter name=\"")
                    .ok_or("Invalid Trae parameter markup")?;
                let (key, after) = after
                    .split_once("\">")
                    .ok_or("Invalid Trae parameter attribute")?;
                let (raw, tail) = after
                    .split_once("</parameter>")
                    .ok_or("Incomplete Trae parameter")?;
                if key.is_empty() || args.contains_key(key) {
                    return Err("Duplicate or empty Trae parameter".into());
                }
                let decoded = raw
                    .replace("&lt;", "<")
                    .replace("&gt;", ">")
                    .replace("&quot;", "\"")
                    .replace("&apos;", "'")
                    .replace("&amp;", "&");
                args.insert(key.to_owned(), self.argument(name, key, &decoded)?);
                rest = tail.trim_start();
            }
        } else {
            return Err("Unsupported or incomplete Trae tool markup".into());
        };
        if !args.is_object() || !self.names.iter().any(|n| n == &name) {
            return Err("Unknown Trae tool or invalid arguments".into());
        }
        Ok((name, args, tail))
    }
}
// serde_json::Value normally accepts duplicate object keys. Tool arguments must
// reject them so a later parser cannot execute a different value.
struct Strict(Value);
impl<'de> serde::Deserialize<'de> for Strict {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Strict;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("JSON without duplicate keys")
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Strict, E> {
                Ok(Strict(json!(v)))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Strict, E> {
                Ok(Strict(json!(v)))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Strict, E> {
                Ok(Strict(json!(v)))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Strict, E> {
                Ok(Strict(json!(v)))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Strict, E> {
                Ok(Strict(json!(v)))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Strict, E> {
                Ok(Strict(Value::Null))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut a: A) -> Result<Strict, A::Error> {
                let mut out = Vec::new();
                while let Some(v) = a.next_element::<Strict>()? {
                    out.push(v.0);
                }
                Ok(Strict(Value::Array(out)))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut a: A) -> Result<Strict, A::Error> {
                let mut out = serde_json::Map::new();
                while let Some(k) = a.next_key::<String>()? {
                    if out.contains_key(&k) {
                        return Err(serde::de::Error::custom("duplicate JSON key"));
                    }
                    out.insert(k, a.next_value::<Strict>()?.0);
                }
                Ok(Strict(Value::Object(out)))
            }
        }
        d.deserialize_any(Visitor)
    }
}
pub(super) fn strict_json(raw: &str) -> Result<Value, String> {
    serde_json::from_str::<Strict>(raw)
        .map(|v| v.0)
        .map_err(|_| "Invalid or duplicate Trae tool JSON".into())
}
