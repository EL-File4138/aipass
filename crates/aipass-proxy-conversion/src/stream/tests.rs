use super::*;
use serde_json::Value;
use std::collections::HashMap;

fn convert_all(converter: &mut StreamConverter, events: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for event in events {
        out.extend(converter.push_event(event).unwrap());
    }
    out
}

/// Parse emitted SSE events into (kind, data-json) pairs.
fn parsed(events: &[String]) -> Vec<(Option<String>, Value)> {
    events
        .iter()
        .map(|raw| {
            let event = parse_sse(raw).unwrap();
            let data = serde_json::from_str(&event.data).unwrap_or(Value::Null);
            (event.kind, data)
        })
        .collect()
}

fn kinds(events: &[String]) -> Vec<String> {
    parsed(events)
        .iter()
        .map(|(kind, _)| kind.clone().unwrap_or_default())
        .collect()
}

// --- CC -> AM ----------------------------------------------------------

#[test]
fn cc_text_stream_becomes_am_event_sequence() {
    let mut c = StreamConverter::new(CC, AM).unwrap();
    let out = convert_all(
        &mut c,
        &[
            "data: {\"id\":\"chatcmpl_1\",\"model\":\"gpt-5\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hel\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl_1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"lo\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl_1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":2}}\n\n",
            "data: [DONE]\n\n",
        ],
    );
    assert_eq!(
        kinds(&out),
        vec![
            "message_start",
            "content_block_start",
            "content_block_delta",
            "content_block_delta",
            "content_block_stop",
            "message_delta",
            "message_stop",
        ]
    );
    let events = parsed(&out);
    assert_eq!(events[0].1["message"]["id"], "msg_1");
    assert_eq!(events[0].1["message"]["model"], "gpt-5");
    assert_eq!(
        events[2].1["delta"],
        json!({"type": "text_delta", "text": "Hel"})
    );
    assert_eq!(
        events[3].1["delta"],
        json!({"type": "text_delta", "text": "lo"})
    );
    assert_eq!(events[5].1["delta"]["stop_reason"], "end_turn");
    assert_eq!(events[5].1["usage"]["output_tokens"], 2);
}

#[test]
fn cc_tool_call_stream_with_fragmented_arguments() {
    let mut c = StreamConverter::new(CC, AM).unwrap();
    let out = convert_all(
        &mut c,
        &[
            "data: {\"id\":\"chatcmpl_1\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"f\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl_1\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\"}}]},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl_1\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"a\\\":1}\"}}]},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl_1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n",
        ],
    );
    assert_eq!(
        kinds(&out),
        vec![
            "message_start",
            "content_block_start",
            "content_block_delta",
            "content_block_stop",
            "message_delta",
            "message_stop",
        ]
    );
    let events = parsed(&out);
    assert_eq!(
        events[1].1["content_block"],
        json!({"type": "tool_use", "id": "call_1", "name": "f", "input": {}})
    );
    assert_eq!(
        events[2].1["delta"],
        json!({"type":"input_json_delta","partial_json":"{\"a\":1}"})
    );
    assert_eq!(events[4].1["delta"]["stop_reason"], "tool_use");
}

#[test]
fn cc_done_without_finish_reason_is_not_success() {
    let mut c = StreamConverter::new(CC, AM).unwrap();
    c.push_event("data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n")
        .unwrap();
    assert!(c.push_event("data: [DONE]\n\n").is_err());
    assert!(c.finish().is_err());
}

#[test]
fn cc_multiline_data_is_merged() {
    let mut c = StreamConverter::new(CC, AM).unwrap();
    let out = c
        .push_event("data: {\ndata: \"id\":\"chatcmpl_1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"x\"}}]\ndata: }\n\n")
        .unwrap();
    assert_eq!(
        kinds(&out),
        vec![
            "message_start",
            "content_block_start",
            "content_block_delta"
        ]
    );
}

// --- AM -> CC ----------------------------------------------------------

#[test]
fn am_text_stream_becomes_cc_chunks() {
    let mut c = StreamConverter::new(AM, CC).unwrap();
    let out = convert_all(
        &mut c,
        &[
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"claude-sonnet-4-5\",\"usage\":{\"input_tokens\":9,\"output_tokens\":0}}}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n\n",
            "event: ping\ndata: {\"type\":\"ping\"}\n\n",
            "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":4}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        ],
    );
    assert_eq!(out.len(), 4);
    let first = parsed(&out[..1]);
    assert_eq!(first[0].1["id"], "chatcmpl_1");
    assert_eq!(first[0].1["model"], "claude-sonnet-4-5");
    assert_eq!(
        first[0].1["choices"][0]["delta"],
        json!({"role": "assistant"})
    );

    let delta: Value = serde_json::from_str(&parse_sse(&out[1]).unwrap().data).unwrap();
    assert_eq!(delta["choices"][0]["delta"], json!({"content": "Hi"}));

    let finish: Value = serde_json::from_str(&parse_sse(&out[2]).unwrap().data).unwrap();
    assert_eq!(finish["choices"][0]["finish_reason"], "stop");
    assert_eq!(
        finish["usage"],
        json!({"prompt_tokens": 9, "completion_tokens": 4, "total_tokens": 13, "prompt_tokens_details":{"cached_tokens":0,"cache_creation_tokens":0}})
    );

    assert_eq!(out[3], "data: [DONE]\n\n");
}

#[test]
fn am_tool_use_stream_becomes_cc_tool_call_fragments() {
    let mut c = StreamConverter::new(AM, CC).unwrap();
    let out = convert_all(
        &mut c,
        &[
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"m\",\"usage\":{\"input_tokens\":1}}}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"f\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\"}}\n\n",
            "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_2\",\"name\":\"g\"}}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":5}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        ],
    );
    let chunks: Vec<Value> = out
        .iter()
        .filter(|raw| !raw.contains("[DONE]"))
        .map(|raw| serde_json::from_str(&parse_sse(raw).unwrap().data).unwrap())
        .collect();
    assert_eq!(
        chunks[1]["choices"][0]["delta"]["tool_calls"][0],
        json!({"index": 0, "id": "toolu_1", "type": "function", "function": {"name": "f", "arguments": ""}})
    );
    assert_eq!(
        chunks[2]["choices"][0]["delta"]["tool_calls"][0],
        json!({"index": 0, "function": {"arguments": "{"}})
    );
    // second tool block gets the next CC index
    assert_eq!(
        chunks[3]["choices"][0]["delta"]["tool_calls"][0],
        json!({"index": 1, "id": "toolu_2", "type": "function", "function": {"name": "g", "arguments": ""}})
    );
    assert_eq!(chunks[4]["choices"][0]["finish_reason"], "tool_calls");
    assert!(out.last().unwrap().contains("[DONE]"));
}

#[test]
fn am_message_stop_without_message_delta_still_terminates() {
    let mut c = StreamConverter::new(AM, CC).unwrap();
    let out = convert_all(
        &mut c,
        &[
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"m\"}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        ],
    );
    assert_eq!(out.len(), 2);
    assert_eq!(out[1], "data: [DONE]\n\n");
}

#[test]
fn unknown_events_are_ignored() {
    let mut am_cc = StreamConverter::new(AM, CC).unwrap();
    assert!(am_cc
        .push_event("event: some_future_event\ndata: {\"type\":\"some_future_event\"}\n\n")
        .unwrap()
        .is_empty());

    let mut rs_am = StreamConverter::new(RS, AM).unwrap();
    assert!(rs_am.push_event("event: response.reasoning.delta\ndata: {\"type\":\"response.reasoning.delta\",\"delta\":\"hmm\"}\n\n").unwrap().is_empty());

    let mut cc_am = StreamConverter::new(CC, AM).unwrap();
    assert!(cc_am.push_event("data: {\"id\":\"chatcmpl_1\",\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":0}}\n\n").unwrap().is_empty());
}

// --- RS -> AM ----------------------------------------------------------

#[test]
fn rs_tool_stream_becomes_am_tool_use() {
    let mut c = StreamConverter::new(RS, AM).unwrap();
    let out = convert_all(
        &mut c,
        &[
            "event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5\",\"status\":\"in_progress\"}}\n\n",
            "event: response.output_item.added\ndata: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"id\":\"fc_1\",\"call_id\":\"call_1\",\"name\":\"f\",\"arguments\":\"\"}}\n\n",
            "event: response.function_call_arguments.delta\ndata: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"fc_1\",\"output_index\":0,\"delta\":\"{\\\"a\\\"\"}\n\n",
            "event: response.function_call_arguments.delta\ndata: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"fc_1\",\"output_index\":0,\"delta\":\":1}\"}\n\n",
            "event: response.output_item.done\ndata: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"id\":\"fc_1\",\"call_id\":\"call_1\",\"name\":\"f\",\"arguments\":\"{\\\"a\\\":1}\"}}\n\n",
            "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"status\":\"completed\",\"usage\":{\"input_tokens\":11,\"output_tokens\":6}}}\n\n",
        ],
    );
    assert_eq!(
        kinds(&out),
        vec![
            "message_start",
            "content_block_start",
            "content_block_delta",
            "content_block_stop",
            "message_delta",
            "message_stop",
        ]
    );
    let events = parsed(&out);
    assert_eq!(events[0].1["message"]["id"], "msg_1");
    assert_eq!(
        events[1].1["content_block"],
        json!({"type": "tool_use", "id": "call_1", "name": "f", "input": {}})
    );
    assert_eq!(
        events[2].1["delta"],
        json!({"type":"input_json_delta","partial_json":"{\"a\":1}"})
    );
    assert_eq!(events[4].1["delta"]["stop_reason"], "tool_use");
    assert_eq!(events[4].1["usage"]["output_tokens"], 6);
}

#[test]
fn rs_text_and_incomplete_status() {
    let mut c = StreamConverter::new(RS, AM).unwrap();
    let out = convert_all(
        &mut c,
        &[
            "event: response.in_progress\ndata: {\"type\":\"response.in_progress\",\"response\":{\"id\":\"resp_2\",\"model\":\"gpt-5\"}}\n\n",
            "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"delta\":\"abc\"}\n\n",
            "event: response.output_text.done\ndata: {\"type\":\"response.output_text.done\",\"output_index\":0,\"text\":\"abc\"}\n\n",
            "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_2\",\"status\":\"incomplete\",\"usage\":{\"input_tokens\":2,\"output_tokens\":50}}}\n\n",
        ],
    );
    let events = parsed(&out);
    assert_eq!(events[0].1["message"]["id"], "msg_2");
    assert_eq!(
        events[2].1["delta"],
        json!({"type": "text_delta", "text": "abc"})
    );
    let last_delta = &events[events.len() - 2];
    assert_eq!(last_delta.1["delta"]["stop_reason"], "max_tokens");
}

// --- AM -> RS ----------------------------------------------------------

#[test]
fn am_stream_becomes_rs_events() {
    let mut c = StreamConverter::new(AM, RS).unwrap();
    let out = convert_all(
        &mut c,
        &[
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"claude-sonnet-4-5\",\"usage\":{\"input_tokens\":8}}}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n\n",
            "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":3}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        ],
    );
    assert_eq!(
        kinds(&out),
        vec![
            "response.created",
            "response.in_progress",
            "response.output_item.added",
            "response.content_part.added",
            "response.output_text.delta",
            "response.output_text.done",
            "response.content_part.done",
            "response.output_item.done",
            "response.completed",
        ]
    );
    let events = parsed(&out);
    assert_eq!(events[0].1["response"]["id"], "resp_1");
    assert_eq!(events[0].1["response"]["status"], "in_progress");
    assert_eq!(events[4].1["delta"], "Hi");
    assert_eq!(events[5].1["text"], "Hi");
    assert_eq!(events[8].1["response"]["status"], "completed");
    assert_eq!(
        events[8].1["response"]["usage"],
        json!({"input_tokens": 8, "output_tokens": 3, "total_tokens": 11, "input_tokens_details":{"cached_tokens":0,"cache_creation_tokens":0}})
    );
}

#[test]
fn am_tool_use_stream_becomes_rs_function_call_items() {
    let mut c = StreamConverter::new(AM, RS).unwrap();
    let out = convert_all(
        &mut c,
        &[
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"m\"}}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"f\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"a\\\"\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\":1}\"}}\n\n",
            "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        ],
    );
    assert_eq!(
        kinds(&out),
        vec![
            "response.created",
            "response.in_progress",
            "response.output_item.added",
            "response.function_call_arguments.delta",
            "response.function_call_arguments.delta",
            "response.function_call_arguments.done",
            "response.output_item.done",
            "response.completed",
        ]
    );
    let events = parsed(&out);
    assert_eq!(
        events[2].1["item"],
        json!({"type": "function_call", "id": "resp_1_fc_0", "call_id": "toolu_1", "name": "f", "arguments": "", "status":"in_progress"})
    );
    // arguments are accumulated into the done item
    assert_eq!(events[5].1["arguments"], "{\"a\":1}");
    assert_eq!(events[6].1["item"]["arguments"], "{\"a\":1}");
    assert_eq!(events[6].1["item"]["status"], "completed");
    assert_eq!(events[7].1["response"]["output"][0], events[6].1["item"]);
    // terminal fallback: message_stop without message_delta still completes
    assert_eq!(events[7].1["response"]["usage"]["output_tokens"], 0);
}

#[test]
fn missing_tool_call_ids_are_synthesized() {
    // Upstreams may omit tool_call/tool_use ids; emitted items must still
    // carry usable ids so the client's tool results can pair back up.
    let mut c = StreamConverter::new(CC, AM).unwrap();
    let out = convert_all(
        &mut c,
        &[
            "data: {\"id\":\"chatcmpl_1\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"type\":\"function\",\"function\":{\"name\":\"f\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl_1\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{}\"}}]},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl_1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n",
        ],
    );
    let events = parsed(&out);
    assert_eq!(events[1].1["content_block"]["id"], "toolu_conv_0");

    let mut c = StreamConverter::new(AM, CC).unwrap();
    let out = convert_all(
        &mut c,
        &[
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"m\"}}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"name\":\"f\"}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        ],
    );
    let chunk: Value = serde_json::from_str(&parse_sse(&out[1]).unwrap().data).unwrap();
    assert_eq!(
        chunk["choices"][0]["delta"]["tool_calls"][0]["id"],
        "call_conv_0"
    );

    let mut c = StreamConverter::new(AM, RS).unwrap();
    let out = convert_all(
        &mut c,
        &[
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"m\"}}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"name\":\"f\"}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        ],
    );
    let events = parsed(&out);
    assert_eq!(events[2].1["item"]["call_id"], "call_conv_0");
}

#[test]
fn upstream_stream_errors_surface_as_terminal_events() {
    // CC mid-stream error payload -> AM error event, nothing after it.
    let mut c = StreamConverter::new(CC, AM).unwrap();
    let out = convert_all(
        &mut c,
        &[
            "data: {\"id\":\"chatcmpl_1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"}}]}\n\n",
            "data: {\"id\":\"chatcmpl_1\",\"error\":{\"type\":\"server_error\",\"message\":\"boom\"}}\n\n",
            "data: [DONE]\n\n",
        ],
    );
    let events = parsed(&out);
    assert_eq!(events.last().unwrap().0.as_deref(), Some("error"));
    assert_eq!(events.last().unwrap().1["error"]["type"], "server_error");
    assert!(!kinds(&out).contains(&"message_stop".to_string()));

    // RS response.failed -> AM error event.
    let mut c = StreamConverter::new(RS, AM).unwrap();
    let out = convert_all(
        &mut c,
        &[
            "event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"status\":\"in_progress\"}}\n\n",
            "event: response.failed\ndata: {\"type\":\"response.failed\",\"response\":{\"id\":\"resp_1\",\"status\":\"failed\",\"error\":{\"code\":\"server_error\",\"message\":\"boom\"}}}\n\n",
        ],
    );
    let events = parsed(&out);
    assert_eq!(events.last().unwrap().0.as_deref(), Some("error"));
    assert_eq!(events.last().unwrap().1["error"]["code"], "server_error");

    // AM error -> CC error chunk + [DONE].
    let mut c = StreamConverter::new(AM, CC).unwrap();
    let out = convert_all(
        &mut c,
        &[
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"m\"}}\n\n",
            "event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"busy\"}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        ],
    );
    assert_eq!(out.last().unwrap(), "data: [DONE]\n\n");
    let chunk: Value = serde_json::from_str(&parse_sse(&out[1]).unwrap().data).unwrap();
    assert_eq!(chunk["error"]["type"], "overloaded_error");
    assert_eq!(out.len(), 3); // role chunk, error chunk, [DONE]

    // AM error -> RS error + response.failed; a trailing message_stop
    // must not emit response.completed afterwards.
    let mut c = StreamConverter::new(AM, RS).unwrap();
    let out = convert_all(
        &mut c,
        &[
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"m\"}}\n\n",
            "event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"busy\"}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        ],
    );
    let events = parsed(&out);
    assert_eq!(events[2].0.as_deref(), Some("error"));
    assert_eq!(events[3].0.as_deref(), Some("response.failed"));
    assert_eq!(events[3].1["response"]["status"], "failed");
    assert!(!kinds(&out).contains(&"response.completed".to_string()));
}

#[test]
fn cc_nonstandard_deltas_and_custom_calls_are_preserved() {
    let mut c = StreamConverter::new(CC, AM).unwrap();
    let out = convert_all(
        &mut c,
        &[
            // structured content parts inside a delta
            "data: {\"id\":\"c\",\"choices\":[{\"index\":0,\"delta\":{\"content\":[{\"type\":\"text\",\"text\":\"hi\"}]}}]}\n\n",
            // freeform custom call: name under custom.name
            "data: {\"id\":\"c\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"custom\",\"custom\":{\"name\":\"apply_patch\"}}]}}]}\n\n",
            "data: {\"id\":\"c\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"custom\":{\"input\":\"*** patch\"}}]}}]}\n\n",
            // a second, regular call whose arguments arrive as an object
            "data: {\"id\":\"c\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":1,\"id\":\"call_2\",\"type\":\"function\",\"function\":{\"name\":\"f\",\"arguments\":{\"a\":1}}}]}}]}\n\n",
            "data: {\"id\":\"c\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n",
        ],
    );
    let events = parsed(&out);
    assert_eq!(
        events[2].1["delta"],
        json!({"type": "text_delta", "text": "hi"})
    );
    assert_eq!(
        events[4].1["content_block"],
        json!({"type": "tool_use", "id": "call_1", "name": "apply_patch", "input": {}})
    );
    // freeform input is wrapped so accumulated partial_json parses
    assert_eq!(
        events[5].1["delta"],
        json!({"type": "input_json_delta", "partial_json": "{\"input\":\"*** patch\"}"})
    );
    assert_eq!(
        events[7].1["content_block"],
        json!({"type": "tool_use", "id": "call_2", "name": "f", "input": {}})
    );
    assert_eq!(
        events[8].1["delta"],
        json!({"type": "input_json_delta", "partial_json": "{\"a\":1}"})
    );
}

#[test]
fn parallel_tools_have_disjoint_block_lifetimes_and_trailing_usage() {
    let mut converter = StreamConverter::new(CC, AM).unwrap();
    let mut out = Vec::new();
    for value in [
        json!({"id":"chatcmpl-1","choices":[{"delta":{"tool_calls":[{"index":0,"id":"a","function":{"name":"one","arguments":"{"}},{"index":1,"id":"b","function":{"name":"two","arguments":"{"}}]}}]}),
        json!({"choices":[{"delta":{"tool_calls":[{"index":1,"function":{"arguments":"\"b\":2}"}},{"index":0,"function":{"arguments":"\"a\":1}"}}]},"finish_reason":"tool_calls"}]}),
        json!({"choices":[],"usage":{"prompt_tokens":30,"completion_tokens":5,"prompt_tokens_details":{"cached_tokens":20}}}),
    ] {
        out.extend(converter.push_event(&format!("data: {value}\n\n")).unwrap());
    }
    out.extend(converter.finish().unwrap());
    let mut open = None;
    let mut args = HashMap::new();
    for (_, event) in parsed(&out) {
        match event["type"].as_str() {
            Some("content_block_start") => {
                assert!(open.is_none());
                open = event["index"].as_u64();
            }
            Some("content_block_delta") => {
                assert_eq!(event["index"].as_u64(), open);
                args.insert(
                    open.unwrap(),
                    event["delta"]["partial_json"].as_str().unwrap().to_owned(),
                );
            }
            Some("content_block_stop") => {
                assert_eq!(event["index"].as_u64(), open);
                open = None;
            }
            Some("message_delta") => {
                assert_eq!(event["usage"]["input_tokens"], 10);
                assert_eq!(event["usage"]["cache_read_input_tokens"], 20);
                assert_eq!(event["usage"]["output_tokens"], 5);
            }
            _ => {}
        }
    }
    assert_eq!(
        serde_json::from_str::<Value>(&args[&0]).unwrap(),
        json!({"a":1})
    );
    assert_eq!(
        serde_json::from_str::<Value>(&args[&1]).unwrap(),
        json!({"b":2})
    );
    for (from, to) in [(AM, CC), (AM, RS), (RS, AM)] {
        assert!(StreamConverter::new(from, to).unwrap().finish().is_err());
    }
}

// --- framing & passthrough ----------------------------------------------

#[test]
fn same_protocol_stream_is_lossless() {
    let mut c = StreamConverter::new(AM, AM).unwrap();
    let event = "event: ping\ndata: {\"type\":\"ping\"}\r\n\r\n";
    assert_eq!(c.push_event(event).unwrap(), vec![event]);
}

#[test]
fn openai_stream_pairs_are_supported() {
    assert!(StreamConverter::new(CC, RS).is_ok());
    assert!(StreamConverter::new(RS, CC).is_ok());
}

#[test]
fn malformed_stream_events_error_without_panicking() {
    let mut c = StreamConverter::new(CC, AM).unwrap();
    assert!(matches!(
        c.push_event("data: {not json\n\n"),
        Err(ConversionError::InvalidPayload { .. })
    ));
    assert!(matches!(
        c.push_event(""),
        Err(ConversionError::InvalidEvent(_))
    ));
    // payload-less events are ignored, not errors
    assert!(c
        .push_event("event: content_block_stop\n\n")
        .unwrap()
        .is_empty());
}
