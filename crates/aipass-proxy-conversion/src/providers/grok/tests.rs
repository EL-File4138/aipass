use super::*;

fn values(chunks: Vec<Vec<u8>>) -> Vec<Value> {
    chunks
        .into_iter()
        .filter_map(|chunk| {
            String::from_utf8(chunk)
                .unwrap()
                .lines()
                .find_map(|line| line.strip_prefix("data: "))
                .and_then(|s| serde_json::from_str(s).ok())
        })
        .collect()
}
fn wire(events: &[Value]) -> Vec<u8> {
    events.iter().flat_map(encode).collect()
}

#[test]
fn namespaces_custom_tools_and_history_are_reversible_without_dropping_tools() {
    let mut body = json!({"tools":[{"type":"namespace","name":"team","tools":[{"type":"function","name":"run","parameters":{"type":"object"}},{"type":"custom","name":"patch","format":{"type":"text"}}]},{"type":"function","name":"team__run","parameters":{"type":"object"}}],"tool_choice":{"type":"function","namespace":"team","name":"run"},"input":[{"type":"custom_tool_call","call_id":"c","namespace":"team","name":"patch","input":"a\n中文"},{"type":"custom_tool_call_output","call_id":"c","output":"ok"}]});
    let mut adapter = Adapter::prepare(&mut body).unwrap();
    assert_eq!(body["tools"].as_array().unwrap().len(), 3);
    assert_eq!(body["tool_choice"]["name"], body["tools"][0]["name"]);
    assert_ne!(body["tools"][0]["name"], body["tools"][2]["name"]);
    assert_eq!(body["input"][0]["name"], body["tools"][1]["name"]);
    assert!(body["tools"][0]["parameters"].get("pattern").is_none());
    assert_eq!(body["input"][1]["type"], "function_call_output");
    let args = json!({"input":"a\n中文"}).to_string();
    let name = &body["tools"][1]["name"];
    let events = [
        json!({"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"item","call_id":"c","name":name,"arguments":""}}),
        json!({"type":"response.function_call_arguments.delta","item_id":"item","output_index":0,"delta":args}),
        json!({"type":"response.function_call_arguments.done","item_id":"item","output_index":0,"arguments":args}),
        json!({"type":"response.output_item.done","output_index":0,"item":{"type":"function_call","id":"item","call_id":"c","name":name,"arguments":args}}),
        json!({"type":"response.completed","response":{"output":[{"type":"function_call","id":"item","call_id":"c","name":name,"arguments":args}]}}),
    ];
    let bytes = wire(&events);
    let mut out = Vec::new();
    for byte in bytes {
        out.extend(adapter.push(&[byte]).unwrap());
    }
    out.extend(adapter.finish().unwrap());
    let events = values(out);
    assert_eq!(events[0]["item"]["type"], "custom_tool_call");
    assert_eq!(events[0]["item"]["name"], "patch");
    assert_eq!(events[0]["item"]["namespace"], "team");
    assert_eq!(
        events
            .iter()
            .filter(|v| v["type"] == "response.custom_tool_call_input.delta")
            .count(),
        1
    );
    assert_eq!(events[1]["delta"], "a\n中文");
    assert_eq!(
        events.last().unwrap()["response"]["output"][0]["input"],
        "a\n中文"
    );
    assert!(events[0].get("response").is_none());
}

#[test]
fn integral_arguments_match_deltas_and_done_for_interleaved_calls_at_every_cut() {
    let original = r#"{"n":9007199254740993.00,"s":"2500.0\\\"","f":2.50,"e":2.0e3,"z":-0.0}"#;
    let expected = r#"{"n":9007199254740993,"s":"2500.0\\\"","f":2.50,"e":2.0e3,"z":-0}"#;
    for split in 0..=original.len() {
        let mut body = json!({});
        let mut adapter = Adapter::prepare(&mut body).unwrap();
        let events = [
            json!({"type":"response.function_call_arguments.delta","item_id":"a","delta":&original[..split]}),
            json!({"type":"response.function_call_arguments.delta","item_id":"b","delta":"{\"n\":12.00}"}),
            json!({"type":"response.function_call_arguments.delta","item_id":"a","delta":&original[split..]}),
            json!({"type":"response.function_call_arguments.done","item_id":"a","arguments":original}),
        ];
        let output = values(adapter.push(&wire(&events)).unwrap());
        let args = output
            .iter()
            .filter(|e| {
                e["type"] == "response.function_call_arguments.delta" && e["item_id"] == "a"
            })
            .filter_map(|e| e["delta"].as_str())
            .collect::<String>();
        assert_eq!(args, expected, "cut {split}");
        assert_eq!(output.last().unwrap()["arguments"], expected);
    }
}

#[test]
fn schema_walk_changes_patterns_only_and_rejects_unrepresentable_tools() {
    let mut body = json!({"tools":[{"type":"function","name":"test","parameters":{"type":"object","properties":{"s":{"type":"string","pattern":r"^[^\s[\]]+$"}},"default":{"pattern":"[[]"},"oneOf":[{"properties":{"x":{"pattern":"[a&&b--c~~d]"}}}]}}]});
    Adapter::prepare(&mut body).unwrap();
    let schema = &body["tools"][0]["parameters"];
    assert_eq!(schema["properties"]["s"]["pattern"], r"^[^\s\[\]]+$");
    assert_eq!(schema["default"]["pattern"], "[[]");
    assert_eq!(
        schema["oneOf"][0]["properties"]["x"]["pattern"],
        r"[a&\&b-\-c~\~d]"
    );
    assert!(schema.get("additionalProperties").is_none());
    for tool in [
        json!({"type":"custom","name":"x","format":{"type":"grammar"}}),
        json!({"type":"unsupported"}),
    ] {
        assert!(Adapter::prepare(&mut json!({"tools":[tool]})).is_err());
    }
}
