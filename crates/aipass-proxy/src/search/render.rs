//! Completed gateway turns retain the client's requested streaming wire format.
use super::*;
#[derive(Default)]
struct Output {
    bytes: String,
    sequence: usize,
}
fn event(out: &mut Output, mut value: Value) {
    if value["type"]
        .as_str()
        .is_some_and(|t| t.starts_with("response."))
    {
        value["sequence_number"] = json!(out.sequence);
        out.sequence += 1;
    }
    if let Some(kind) = value["type"].as_str() {
        out.bytes.push_str(&format!("event: {kind}\n"));
    }
    out.bytes.push_str(&format!("data: {value}\n\n"));
}
pub(super) fn sse(protocol: ProxyProtocol, body: &Value) -> String {
    let mut out = Output::default();
    match protocol {
        ProxyProtocol::AnthropicMessages => {
            let mut start = body.clone();
            start["content"] = json!([]);
            start["stop_reason"] = Value::Null;
            start["stop_sequence"] = Value::Null;
            event(&mut out, json!({"type":"message_start","message":start}));
            for (i, block) in body["content"].as_array().into_iter().flatten().enumerate() {
                let mut start = block.clone();
                let mut deltas = Vec::new();
                match block["type"].as_str() {
                    Some("text") => {
                        start["text"] = json!("");
                        deltas.push(json!({"type":"text_delta","text":block["text"]}));
                    }
                    Some("thinking") => {
                        start["thinking"] = json!("");
                        start["signature"] = json!("");
                        deltas.push(json!({"type":"thinking_delta","thinking":block["thinking"]}));
                        if let Some(signature) = block.get("signature") {
                            deltas.push(json!({"type":"signature_delta","signature":signature}));
                        }
                    }
                    Some("tool_use") => {
                        start["input"] = json!({});
                        deltas.push(json!({"type":"input_json_delta","partial_json":block["input"].to_string()}));
                    }
                    _ => {}
                }
                event(
                    &mut out,
                    json!({"type":"content_block_start","index":i,"content_block":start}),
                );
                for delta in deltas {
                    event(
                        &mut out,
                        json!({"type":"content_block_delta","index":i,"delta":delta}),
                    );
                }
                event(&mut out, json!({"type":"content_block_stop","index":i}));
            }
            event(
                &mut out,
                json!({"type":"message_delta","delta":{"stop_reason":body["stop_reason"],"stop_sequence":body["stop_sequence"]},"usage":body["usage"]}),
            );
            event(&mut out, json!({"type":"message_stop"}));
        }
        ProxyProtocol::OpenAiChatCompletions => {
            let message = &body["choices"][0]["message"];
            event(
                &mut out,
                json!({"id":body["id"],"object":"chat.completion.chunk","model":body["model"],"created":body["created"],"choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}),
            );
            let mut delta = message.clone();
            delta.as_object_mut().map(|o| o.remove("role"));
            if let Some(calls) = delta["tool_calls"].as_array_mut() {
                for (i, call) in calls.iter_mut().enumerate() {
                    call["index"] = json!(i);
                }
            }
            event(
                &mut out,
                json!({"id":body["id"],"object":"chat.completion.chunk","model":body["model"],"created":body["created"],"choices":[{"index":0,"delta":delta,"finish_reason":null}]}),
            );
            event(
                &mut out,
                json!({"id":body["id"],"object":"chat.completion.chunk","model":body["model"],"created":body["created"],"choices":[{"index":0,"delta":{},"finish_reason":body["choices"][0]["finish_reason"]}],"usage":body["usage"]}),
            );
            out.bytes.push_str("data: [DONE]\n\n");
        }
        ProxyProtocol::OpenAiResponses => {
            let mut start = body.clone();
            start["status"] = json!("in_progress");
            start["output"] = json!([]);
            event(
                &mut out,
                json!({"type":"response.created","response":start}),
            );
            for (i, item) in body["output"].as_array().into_iter().flatten().enumerate() {
                let mut added = item.clone();
                added["status"] = json!("in_progress");
                if item["type"] == "message" {
                    added["content"] = json!([]);
                } else if item["type"] == "function_call" {
                    added["arguments"] = json!("");
                }
                event(
                    &mut out,
                    json!({"type":"response.output_item.added","output_index":i,"item":added}),
                );
                for (j, part) in item["content"].as_array().into_iter().flatten().enumerate() {
                    let mut empty = part.clone();
                    if part["type"] == "output_text" {
                        empty["text"] = json!("");
                    }
                    event(
                        &mut out,
                        json!({"type":"response.content_part.added","item_id":item["id"],"output_index":i,"content_index":j,"part":empty}),
                    );
                    if part["type"] == "output_text" {
                        event(
                            &mut out,
                            json!({"type":"response.output_text.delta","item_id":item["id"],"output_index":i,"content_index":j,"delta":part["text"]}),
                        );
                        event(
                            &mut out,
                            json!({"type":"response.output_text.done","item_id":item["id"],"output_index":i,"content_index":j,"text":part["text"]}),
                        );
                    }
                    event(
                        &mut out,
                        json!({"type":"response.content_part.done","item_id":item["id"],"output_index":i,"content_index":j,"part":part}),
                    );
                }
                if item["type"] == "function_call" {
                    event(
                        &mut out,
                        json!({"type":"response.function_call_arguments.delta","item_id":item["id"],"output_index":i,"delta":item["arguments"]}),
                    );
                    event(
                        &mut out,
                        json!({"type":"response.function_call_arguments.done","item_id":item["id"],"output_index":i,"arguments":item["arguments"]}),
                    );
                }
                event(
                    &mut out,
                    json!({"type":"response.output_item.done","output_index":i,"item":item}),
                );
            }
            let kind = match body["status"].as_str() {
                Some("incomplete") => "response.incomplete",
                Some("failed") => "response.failed",
                _ => "response.completed",
            };
            event(&mut out, json!({"type":kind,"response":body}));
        }
    }
    out.bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_responses_never_report_a_completed_event() {
        let events = sse(
            ProxyProtocol::OpenAiResponses,
            &json!({"id":"resp_failed","status":"failed","output":[],"error":{"code":"server_error"}}),
        );
        assert!(events.contains("event: response.failed\n"));
        assert!(!events.contains("event: response.completed\n"));
    }
    #[test]
    fn caller_tool_and_usage_survive_stream_conversion() {
        use aipass_proxy_conversion::{BuiltinConversionPlugin, ConversionPlugin, StreamConverter};
        let cc = json!({"id":"chatcmpl_one","created":1,"model":"test","choices":[{"index":0,"message":{"role":"assistant","content":null,"tool_calls":[{"id":"call_one","type":"function","function":{"name":"caller_tool","arguments":"{\"path\":\"file.txt\"}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":30,"completion_tokens":7,"total_tokens":37}});
        for protocol in [
            ProxyProtocol::AnthropicMessages,
            ProxyProtocol::OpenAiChatCompletions,
            ProxyProtocol::OpenAiResponses,
        ] {
            let body = BuiltinConversionPlugin
                .convert_response(ProxyProtocol::OpenAiChatCompletions, protocol, cc.clone())
                .unwrap();
            let mut converter =
                StreamConverter::new(protocol, ProxyProtocol::AnthropicMessages).unwrap();
            let mut converted = Vec::new();
            for event in sse(protocol, &body).split("\n\n").filter(|s| !s.is_empty()) {
                converted.extend(converter.push_event(event).unwrap());
            }
            converted.extend(converter.finish().unwrap());
            let output = converted.join("\n");
            assert!(output.contains("caller_tool"), "{protocol:?}: {output}");
            assert!(output.contains("file.txt"));
            assert!(output.contains("message_stop"));
            let mut usage = aipass_proxy_conversion::UsageAccumulator::default();
            for line in output.lines().filter_map(|l| l.strip_prefix("data: ")) {
                let value: Value = serde_json::from_str(line).unwrap();
                usage.observe(ProxyProtocol::AnthropicMessages, &value);
            }
            assert_eq!(
                usage.totals(ProxyProtocol::AnthropicMessages).input_tokens,
                30
            );
            assert_eq!(
                usage.totals(ProxyProtocol::AnthropicMessages).output_tokens,
                7
            );
        }
    }
}
