use super::*;
#[test]
fn content_and_connect_failure_in_one_chunk_is_a_submitted_generation() {
    let mut s = CursorStream::new("m", "id", request(&chat(), "m", "s").unwrap());
    let body = [
        frame(0, &bytes(1, &bytes(1, &string(1, "content")))),
        frame(2, br#"{"error":{"code":"resource_exhausted"}}"#),
    ]
    .concat();
    let batch = s.push(&body).unwrap();
    assert!(batch.error.is_some());
    assert!(!batch.chunks.is_empty());
    assert!(s.has_output());
}
fn chat() -> Value {
    json!({"messages":[{"role":"user","content":"hello"}],"tools":[{"type":"function","function":{"name":"search","parameters":{"type":"object"}}}]})
}
#[test]
fn model_family_keeps_fast_and_thinking_and_uses_nearest_effort() {
    let raw = vec![
        json!({"id":"grok-low","name":"Grok Low","run":"L"}),
        json!({"id":"grok-high","name":"Grok High","run":"H"}),
        json!({"id":"grok-high-fast","name":"Grok High Fast","run":"HF"}),
    ];
    assert_eq!(
        split_model("claude-thinking-extra-high-fast"),
        ("claude-thinking-fast".into(), "xhigh".into())
    );
    assert_eq!(model_id(&raw, "grok", "medium", false), "H");
    assert_eq!(model_id(&raw, "grok", "high", true), "HF");
}
#[test]
fn protobuf_values_reject_lossy_large_integers() {
    let v = json!({"flag":true,"integer":2500,"items":[null,"text",4.5,1e20]});
    assert_eq!(pb_any(&pb_value(&v, 0).unwrap(), 0).unwrap(), v);
    assert!(pb_value(&json!(9007199254740993u64), 0).is_err());
}
#[test]
fn caller_tools_are_returned_and_foreign_execution_is_refused() {
    let run = request(&chat(), "model", "session").unwrap();
    let mut s = CursorStream::new("model", "id", run);
    let unknown = bytes(2, &[uint(1, 1), bytes(11, &string(5, "shell"))].concat());
    let out = s.push(&frame(0, &unknown)).unwrap();
    assert_eq!(out.outgoing.len(), 2);
    assert!(out.chunks.is_empty());
    let args = bytes(
        2,
        &[
            string(1, "q"),
            bytes(2, &pb_value(&json!("needle"), 0).unwrap()),
        ]
        .concat(),
    );
    let call = bytes(
        2,
        &[
            uint(1, 2),
            bytes(
                11,
                &[string(5, "search"), string(3, "call_a\nfc_b"), args].concat(),
            ),
        ]
        .concat(),
    );
    let out = s.push(&frame(0, &call)).unwrap();
    let text = String::from_utf8(out.chunks.concat()).unwrap();
    assert!(text.contains("call_a__fc_b"));
    assert!(text.contains("needle"));
    assert!(s.finish().is_err());
    let count = bytes(1, &bytes(27, &uint(1, 1)));
    s.push(&frame(0, &count)).unwrap();
    s.finish().unwrap();
}
#[test]
fn completion_preserves_cache_and_reasoning_usage() {
    let mut s = CursorStream::new("m", "id", request(&chat(), "m", "s").unwrap());
    let payload = bytes(
        1,
        &[
            bytes(1, &string(1, "hi")),
            bytes(
                14,
                &[
                    uint(1, 100),
                    uint(2, 7),
                    uint(3, 80),
                    uint(4, 5),
                    uint(5, 3),
                ]
                .concat(),
            ),
        ]
        .concat(),
    );
    let mut out = vec![];
    for b in frame(0, &payload) {
        out.extend(s.push(&[b]).unwrap().chunks);
    }
    s.finish().unwrap();
    let text = String::from_utf8(out.concat()).unwrap();
    assert!(text.contains("\"prompt_tokens\":100"));
    assert!(text.contains("\"cache_creation_tokens\":5"));
    assert!(text.contains("\"reasoning_tokens\":3"));
}
#[test]
fn conversation_is_session_bound_and_repairs_missing_tool_results() {
    let mut c = chat();
    c["messages"] = json!([{"role":"assistant","tool_calls":[{"id":"call_a","type":"function","function":{"name":"search","arguments":"{}"}}]},{"role":"user","content":"continue"}]);
    let run = request(&c, "m", "s").unwrap();
    assert!(run
        .blobs
        .values()
        .any(|b| String::from_utf8_lossy(b).contains("interrupted")));
    c["messages"] = json!([{"role":"tool","tool_call_id":"orphan","content":"x"}]);
    assert!(request(&c, "m", "s").is_err());
}

#[test]
fn retained_batches_validate_results_before_upload_and_keep_native_blob_state() {
    let mut decoder = CursorStream::new_retained("m", "first", request(&chat(), "m", "s").unwrap());
    let call = bytes(
        2,
        &[
            uint(1, 7),
            string(15, "exec-7"),
            bytes(11, &[string(5, "search"), string(3, "original")].concat()),
        ]
        .concat(),
    );
    let declared = bytes(1, &bytes(27, &uint(1, 1)));
    let batch = decoder.push(&frame(0, &[call, declared].concat())).unwrap();
    assert!(decoder.parked());
    assert!(!decoder.done());
    assert!(!batch.chunks.is_empty());
    let id = decoder.pending()[0].id.clone();
    assert!(id.starts_with("call_aipass_cursor_"));
    assert!(decoder.resume(&[], "next").is_err());
    assert!(decoder.parked());
    let set = bytes(
        4,
        &[
            uint(1, 8),
            bytes(3, &[bytes(1, b"key"), bytes(2, b"state")].concat()),
        ]
        .concat(),
    );
    assert_eq!(decoder.push(&frame(0, &set)).unwrap().outgoing.len(), 1);
    let get = bytes(4, &[uint(1, 9), bytes(2, &bytes(1, b"key"))].concat());
    assert!(decoder
        .push(&frame(0, &get))
        .unwrap()
        .outgoing
        .concat()
        .windows(5)
        .any(|b| b == b"state"));
    let frames = decoder
        .resume(
            &[json!({"role":"tool","tool_call_id":id,"content":"result"})],
            "next",
        )
        .unwrap();
    assert_eq!(frames.len(), 2);
    assert!(!decoder.parked());
    assert!(decoder.resume(&[], "duplicate").is_err());
    let final_event = bytes(
        1,
        &[
            bytes(1, &string(1, "done")),
            bytes(
                14,
                &[uint(1, 100), uint(2, 7), uint(3, 80), uint(4, 5)].concat(),
            ),
        ]
        .concat(),
    );
    let output = decoder.push(&frame(0, &final_event)).unwrap();
    assert!(decoder.done());
    decoder.finish().unwrap();
    assert!(String::from_utf8(output.chunks.concat())
        .unwrap()
        .contains("cache_creation_tokens"));
}
#[test]
fn retained_session_rejects_cancelled_exec_and_counts_refused_calls_in_batch() {
    let mut decoder = CursorStream::new_retained("m", "first", request(&chat(), "m", "s").unwrap());
    let foreign = bytes(2, &[uint(1, 1), bytes(11, &string(5, "shell"))].concat());
    let own = bytes(2, &[uint(1, 2), bytes(11, &string(5, "search"))].concat());
    let declared = bytes(1, &bytes(27, &uint(1, 2)));
    decoder
        .push(&frame(0, &[foreign, own, declared].concat()))
        .unwrap();
    assert!(decoder.parked());
    let cancelled = bytes(5, &bytes(1, &uint(1, 2)));
    assert!(decoder.push(&frame(0, &cancelled)).is_err());
}
