//! Stateless Codex history adaptations. Caller instructions and tool choices
//! remain authoritative; cached search declarations cannot enable live search.
use serde_json::{json, Map, Value};

pub(super) fn adapt(body: &mut Map<String, Value>) -> Result<(), String> {
    if let Some(reasoning) = body.get_mut("reasoning").and_then(Value::as_object_mut) {
        if reasoning.get("effort").and_then(Value::as_str) == Some("ultra") {
            reasoning.insert("effort".into(), json!("max"));
        }
    }
    if body
        .get("reasoning")
        .and_then(|v| v.get("effort"))
        .and_then(Value::as_str)
        != Some("none")
    {
        let include = body.entry("include").or_insert_with(|| json!([]));
        let include = include
            .as_array_mut()
            .ok_or("Codex include must be an array")?;
        if !include.iter().any(|v| v == "reasoning.encrypted_content") {
            include.push(json!("reasoning.encrypted_content"));
        }
    }
    let Some(input) = body.get("input").and_then(Value::as_array) else {
        return Ok(());
    };
    if !input.iter().any(|v| v["type"] == "web_search_call") {
        return Ok(());
    }
    let declares_search = |tools: Option<&Vec<Value>>| {
        tools.is_some_and(|tools| {
            tools.iter().any(|v| {
                v["type"]
                    .as_str()
                    .is_some_and(|t| t.starts_with("web_search"))
            })
        })
    };
    let tools = body.get("tools").and_then(Value::as_array);
    let additional = input.iter().position(|v| v["type"] == "additional_tools");
    if declares_search(tools)
        || input
            .iter()
            .any(|v| v["type"] == "additional_tools" && declares_search(v["tools"].as_array()))
    {
        return Ok(());
    }
    if body.get("tools").is_some_and(|v| !v.is_array()) {
        return Err("Codex tools must be an array".into());
    }
    let offered = tools.is_some_and(|t| !t.is_empty())
        || input.iter().any(|v| {
            v["type"] == "additional_tools" && v["tools"].as_array().is_some_and(|t| !t.is_empty())
        });
    let cached = json!({"type":"web_search","external_web_access":false});
    if let Some(index) = additional {
        body["input"][index]["tools"]
            .as_array_mut()
            .ok_or("Codex additional tools must be an array")?
            .push(cached);
    } else if body
        .get("reasoning")
        .and_then(|v| v.get("context"))
        .and_then(Value::as_str)
        == Some("all_turns")
    {
        body["input"].as_array_mut().unwrap().insert(
            0,
            json!({"type":"additional_tools","role":"developer","tools":[cached]}),
        );
    } else {
        body.entry("tools")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .unwrap()
            .push(cached);
    }
    if !offered
        && body
            .get("tool_choice")
            .is_none_or(|v| v.is_null() || matches!(v.as_str(), Some("auto" | "none" | "")))
    {
        body.insert("tool_choice".into(), json!("none"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn prepare(body: Value) -> Value {
        serde_json::from_slice(
            &super::super::prepare(bytes::Bytes::from(body.to_string())).unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn stateless_reasoning_preserves_requested_includes_and_explicit_disabled_reasoning() {
        let v = prepare(
            json!({"input":[],"reasoning":{"effort":"ultra"},"include":["web_search_call.action.sources"],"prompt_cache_options":{"mode":"explicit"},"service_tier":"ultrafast"}),
        );
        assert_eq!(v["reasoning"]["effort"], "max");
        assert_eq!(
            v["include"],
            json!([
                "web_search_call.action.sources",
                "reasoning.encrypted_content"
            ])
        );
        assert!(v.get("prompt_cache_options").is_none());
        assert_eq!(v["service_tier"], "ultrafast");
        let v = prepare(
            json!({"input":[],"reasoning":{"effort":"none"},"include":["web_search_call.action.sources"]}),
        );
        assert_eq!(v["include"], json!(["web_search_call.action.sources"]));
    }
    #[test]
    fn replayed_search_does_not_enable_new_tools_or_overwrite_callers_choice() {
        let v = prepare(json!({"input":[{"type":"web_search_call","id":"old"}],"tools":[]}));
        assert_eq!(
            v["tools"],
            json!([{"type":"web_search","external_web_access":false}])
        );
        assert_eq!(v["tool_choice"], "none");
        let v = prepare(
            json!({"input":[{"type":"web_search_call"}],"tools":[{"type":"function","name":"lookup"}],"tool_choice":{"type":"function","name":"lookup"}}),
        );
        assert_eq!(v["tool_choice"]["name"], "lookup");
        let v = prepare(
            json!({"input":[{"type":"web_search_call"}],"reasoning":{"context":"all_turns"},"tool_choice":"none"}),
        );
        assert_eq!(v["input"][0]["type"], "additional_tools");
        assert!(v.get("tools").is_none());
        assert_eq!(v["tool_choice"], "none");
    }
    #[test]
    fn native_v2_compaction_is_preserved_as_opaque_vendor_history() {
        let input = json!([{"type":"compaction","id":"cmp_old","encrypted_content":"opaque-compaction"},
            {"type":"reasoning","encrypted_content":"opaque-reasoning"},{"role":"user","content":"continue"}]);
        let v = prepare(
            json!({"model":"test","input":input,"compaction_trigger":{"type":"manual"},"reasoning":{"context":"all_turns"},"instructions":"original"}),
        );
        assert_eq!(v["compaction_trigger"], json!({"type":"manual"}));
        assert_eq!(v["input"][0]["encrypted_content"], "opaque-compaction");
        assert_eq!(v["input"][1]["encrypted_content"], "opaque-reasoning");
        assert_eq!(v["instructions"], "original");
    }
}
