use serde_json::Value;

// Visit schema positions only. An enum, default or example can contain arbitrary
// application objects with a property called "pattern".
pub(super) fn normalize(schema: &mut Value) {
    if let Some(pattern) = schema.get_mut("pattern") {
        if let Some(text) = pattern.as_str() {
            *pattern = Value::String(compatible_pattern(text));
        }
    }
    for key in [
        "properties",
        "patternProperties",
        "$defs",
        "definitions",
        "dependentSchemas",
    ] {
        for value in schema
            .get_mut(key)
            .and_then(Value::as_object_mut)
            .into_iter()
            .flat_map(|m| m.values_mut())
        {
            normalize(value);
        }
    }
    for key in [
        "items",
        "additionalProperties",
        "contains",
        "not",
        "if",
        "then",
        "else",
        "propertyNames",
        "unevaluatedProperties",
    ] {
        if let Some(value) = schema.get_mut(key) {
            if let Some(array) = value.as_array_mut() {
                array.iter_mut().for_each(normalize);
            } else if value.is_object() {
                normalize(value);
            }
        }
    }
    for key in ["allOf", "anyOf", "oneOf", "prefixItems"] {
        for value in schema
            .get_mut(key)
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            normalize(value);
        }
    }
}
fn compatible_pattern(pattern: &str) -> String {
    let mut out = String::new();
    let mut chars = pattern.chars().peekable();
    let mut class = false;
    while let Some(c) = chars.next() {
        if c == '\\' {
            out.push(c);
            if let Some(escaped) = chars.next() {
                out.push(escaped);
            }
            continue;
        }
        match c {
            '[' if !class => class = true,
            ']' if class => class = false,
            '[' if class => out.push('\\'),
            '&' | '-' | '~' if class && chars.peek() == Some(&c) => {
                out.push(c);
                out.push('\\');
                out.push(chars.next().unwrap());
                continue;
            }
            _ => {}
        }
        out.push(c);
    }
    out
}
