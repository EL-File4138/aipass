//! Merge cumulative wire counters before separating cached and uncached input.
use serde_json::{Map, Value};

use crate::{extract_usage, ProxyProtocol, TokenUsage};

/// A stream may report input, output and cache details in different events.
/// Missing/null fields carry no new observation; an explicit zero is a value.
#[derive(Default)]
pub struct UsageAccumulator {
    wire: Value,
}

impl UsageAccumulator {
    pub fn update(&mut self, usage: &Value) {
        merge(&mut self.wire, usage);
    }

    pub fn observe(&mut self, protocol: ProxyProtocol, payload: &Value) {
        let nested = match protocol {
            ProxyProtocol::AnthropicMessages => payload.get("message"),
            ProxyProtocol::OpenAiResponses => payload.get("response"),
            ProxyProtocol::OpenAiChatCompletions => None,
        };
        if let Some(usage) = nested.and_then(|v| v.get("usage")) {
            self.update(usage);
        }
        if let Some(usage) = payload.get("usage") {
            self.update(usage);
        }
    }

    pub fn wire(&self) -> &Value {
        &self.wire
    }

    pub fn totals(&self, protocol: ProxyProtocol) -> TokenUsage {
        extract_usage(protocol, &self.wire)
    }
}

fn merge(total: &mut Value, update: &Value) {
    let Some(fields) = update.as_object() else {
        return;
    };
    if !total.is_object() {
        *total = Value::Object(Map::new());
    }
    for (key, value) in fields {
        if value.is_object() {
            merge(&mut total[key], value);
        } else if value.as_u64().is_some() {
            total[key] = value.clone();
        }
    }
}

#[cfg(test)]
mod tests;
