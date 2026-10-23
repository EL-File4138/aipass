//! Kimi Work's Moonshot tool-schema dialect. Only schema locations are visited.
use serde_json::{json, Value};
const MAPS: &[&str] = &[
    "properties",
    "$defs",
    "definitions",
    "patternProperties",
    "dependentSchemas",
];
const CHILDREN: &[&str] = &[
    "items",
    "additionalProperties",
    "contains",
    "propertyNames",
    "not",
    "if",
    "then",
    "else",
    "allOf",
    "anyOf",
    "oneOf",
    "prefixItems",
    "unevaluatedProperties",
    "unevaluatedItems",
    "contentSchema",
];
const TYPES: &[&str] = &[
    "string", "number", "integer", "boolean", "object", "array", "null",
];

fn paths(schema: &Value) -> Result<Vec<String>, String> {
    fn visit(v: &Value, path: String, depth: usize, out: &mut Vec<String>) -> Result<(), String> {
        if depth > 64 || out.len() >= 16384 {
            return Err("Kimi tool schema exceeds traversal limit".into());
        }
        if let Some(array) = v.as_array() {
            for (i, child) in array.iter().enumerate() {
                visit(child, format!("{path}/{i}"), depth + 1, out)?;
            }
        } else if v.is_object() {
            out.push(path.clone());
            for field in MAPS {
                for (key, child) in v[*field].as_object().into_iter().flatten() {
                    let escaped = key.replace('~', "~0").replace('/', "~1");
                    visit(child, format!("{path}/{field}/{escaped}"), depth + 1, out)?;
                }
            }
            for field in CHILDREN {
                if let Some(child) = v.get(*field) {
                    visit(child, format!("{path}/{field}"), depth + 1, out)?;
                }
            }
        }
        Ok(())
    }
    let mut out = vec![];
    visit(schema, String::new(), 0, &mut out)?;
    Ok(out)
}
fn value_type(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
        Value::Number(n) if n.as_f64().is_some_and(|n| n.fract() == 0.) => "integer",
        Value::Number(_) => "number",
    }
}
fn enum_type(v: &Value) -> Option<Value> {
    let values = v
        .get("const")
        .map(|c| vec![c])
        .or_else(|| v["enum"].as_array().map(|a| a.iter().collect()))?;
    let mut types = std::collections::BTreeSet::new();
    for value in values {
        types.insert(value_type(value));
    }
    if types.contains("number") {
        types.remove("integer");
    }
    if types.contains("object")
        || types.contains("array")
        || types.iter().filter(|t| **t != "null").count() > 1
    {
        return None;
    }
    let names: Vec<_> = TYPES
        .iter()
        .copied()
        .filter(|t| types.contains(t))
        .collect();
    match names.as_slice() {
        [] => None,
        [one] => Some(json!(one)),
        _ => Some(json!(names)),
    }
}
fn inferred_type(v: &Value) -> Value {
    if let Some(t) = enum_type(v) {
        return t;
    }
    for (kind, fields) in [
        (
            "object",
            &[
                "properties",
                "required",
                "additionalProperties",
                "patternProperties",
                "minProperties",
                "maxProperties",
                "dependentSchemas",
            ] as &[&str],
        ),
        (
            "array",
            &["items", "prefixItems", "minItems", "maxItems", "contains"],
        ),
        (
            "string",
            &[
                "minLength",
                "maxLength",
                "pattern",
                "format",
                "contentEncoding",
            ],
        ),
        (
            "number",
            &[
                "minimum",
                "maximum",
                "exclusiveMinimum",
                "exclusiveMaximum",
                "multipleOf",
            ],
        ),
    ] {
        if fields.iter().any(|field| v.get(*field).is_some()) {
            return json!(kind);
        }
    }
    // Keep unconstrained and union targets permissive; branch constraints remain.
    json!(TYPES)
}
fn local_pointer(reference: &str) -> Option<&str> {
    let pointer = reference.strip_prefix('#')?;
    (pointer.is_empty() || pointer.starts_with('/')).then_some(pointer)
}
fn normalize(schema: &mut Value) -> Result<(), String> {
    let original = schema.clone();
    for path in paths(schema)? {
        let node = schema
            .pointer_mut(&path)
            .and_then(Value::as_object_mut)
            .unwrap();
        if node.contains_key("type") {
            if let Some(reference) = node.get("$ref").and_then(Value::as_str) {
                let target = local_pointer(reference).and_then(|p| original.pointer(p));
                let expandable = match target {
                    Some(v) if v.is_object() => !paths(v)?
                        .iter()
                        .any(|p| v.pointer(p).unwrap().get("$ref").is_some()),
                    _ => false,
                };
                if expandable {
                    let mut merged = target.unwrap().as_object().unwrap().clone();
                    node.remove("$ref");
                    merged.extend(std::mem::take(node));
                    *node = merged;
                } else {
                    node.remove("type");
                }
            }
            if node.get("anyOf").is_some_and(Value::is_array) {
                node.remove("type");
            }
        }
    }
    for path in paths(schema)? {
        let node = schema.pointer_mut(&path).unwrap();
        if node.get("type").is_none() && node.get("$ref").is_none() && node.get("anyOf").is_none() {
            if let Some(t) = enum_type(node) {
                node["type"] = t;
            }
        }
    }
    let targets: Vec<String> = paths(schema)?
        .iter()
        .filter_map(|p| {
            schema.pointer(p)?["$ref"]
                .as_str()
                .and_then(local_pointer)
                .map(str::to_owned)
        })
        .collect();
    for target in targets {
        let Some(node) = schema.pointer_mut(&target) else {
            continue;
        };
        if node.is_object()
            && !node.as_object().unwrap().is_empty()
            && ["type", "$ref", "anyOf"]
                .iter()
                .all(|key| node.get(*key).is_none())
        {
            node["type"] = inferred_type(node);
        }
    }
    Ok(())
}
pub fn normalize_tools(body: &mut Value) -> Result<(), String> {
    for tool in body
        .get_mut("tools")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        if let Some(parameters) = tool
            .pointer_mut("/function/parameters")
            .filter(|p| p.is_object())
        {
            normalize(parameters)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn derived_refs_keep_constraints_and_only_tool_schemas_change() {
        let mut body = json!({"messages":[{"role":"user","content":"keep $ref text"}],"tools":[{"type":"function","function":{"name":"run","parameters":{"type":"object","properties":{"n":{"type":"integer","$ref":"#/$defs/nonnegative","maximum":9000},"tag":{"enum":["a",null]},"choice":{"type":"string","anyOf":[{"type":"string"},{"type":"null"}]}},"$defs":{"nonnegative":{"type":"number","minimum":0}},"default":{"type":"string","$ref":"literal data"}}}}]});
        let original = body.clone();
        normalize_tools(&mut body).unwrap();
        let p = &body["tools"][0]["function"]["parameters"];
        assert_eq!(
            p["properties"]["n"],
            json!({"type":"integer","minimum":0,"maximum":9000})
        );
        assert_eq!(p["properties"]["tag"]["type"], json!(["string", "null"]));
        assert!(p["properties"]["choice"].get("type").is_none());
        assert_eq!(
            p["default"],
            original["tools"][0]["function"]["parameters"]["default"]
        );
        assert_eq!(body["messages"], original["messages"]);
        let once = body.clone();
        normalize_tools(&mut body).unwrap();
        assert_eq!(body, once);
    }
    #[test]
    fn recursive_refs_stay_bounded_and_reference_targets_get_types() {
        let mut schema = json!({"type":"object","properties":{"n":{"type":"object","$ref":"#/$defs/node"},"name":{"$ref":"#/$defs/a~1b"}},"$defs":{"node":{"type":"object","properties":{"next":{"$ref":"#/$defs/node"}}},"a/b":{"minLength":1}}});
        normalize(&mut schema).unwrap();
        assert_eq!(schema["properties"]["n"], json!({"$ref":"#/$defs/node"}));
        assert_eq!(schema["$defs"]["a/b"]["type"], "string");
        let mut deep = json!({"type":"string"});
        for _ in 0..66 {
            deep = json!({"items":deep});
        }
        assert!(normalize(&mut deep).is_err());
    }
    #[test]
    fn valid_tool_definitions_are_unchanged() {
        let mut body = json!({"tools":[{"function":{"parameters":{"type":"object","properties":{"n":{"$ref":"#/$defs/n"}},"$defs":{"n":{"type":"integer"}}}}}]});
        let original = body.clone();
        normalize_tools(&mut body).unwrap();
        assert_eq!(body, original);
        let mut body = json!({"messages":[{"role":"user","content":"hi"}]});
        let original = body.clone();
        normalize_tools(&mut body).unwrap();
        assert_eq!(body, original);
    }
}
