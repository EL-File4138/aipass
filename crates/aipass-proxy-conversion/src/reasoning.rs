//! Plain reasoning exposed by compatible APIs, including Mistral typed parts.
use serde_json::Value;

/// Ciphertext in Responses wire items, never arbitrary JSON tool payloads.
/// Read both item-level state and explicitly typed encrypted content blocks.
pub fn response_item_ciphertexts(item: &Value) -> Vec<&str> {
    let mut values = Vec::new();
    if let Some(value) = item["encrypted_content"].as_str().filter(|s| !s.is_empty()) {
        values.push(value);
    }
    if item["type"] == "encrypted_content" {
        if let Some(value) = item["data"].as_str().filter(|s| !s.is_empty()) {
            values.push(value);
        }
    }
    let parts = match item["type"].as_str() {
        Some("message" | "agent_message") => item.get("content"),
        Some("function_call_output" | "custom_tool_call_output") => item.get("output"),
        None if item["role"].is_string() => item.get("content"),
        _ => None,
    };
    for part in parts.and_then(Value::as_array).into_iter().flatten() {
        if part["type"] == "encrypted_content" {
            for key in ["data", "encrypted_content"] {
                if let Some(value) = part[key].as_str().filter(|s| !s.is_empty()) {
                    values.push(value);
                }
            }
        }
    }
    values
}

pub(crate) fn chat_reasoning(message: &Value) -> String {
    let mut text = message
        .get("reasoning_content")
        .or_else(|| message.get("reasoning"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    if let Some(parts) = message.get("content").and_then(Value::as_array) {
        for part in parts {
            if part["type"] == "thinking" {
                match part.get("thinking") {
                    Some(Value::String(value)) => text.push_str(value),
                    Some(Value::Array(values)) => {
                        for value in values {
                            if let Some(value) = value.get("text").and_then(Value::as_str) {
                                text.push_str(value);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    text
}

pub(crate) fn anthropic_reasoning(content: Option<&Value>) -> String {
    content
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter(|p| p["type"] == "thinking")
                .filter_map(|p| p.get("thinking").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default()
}
