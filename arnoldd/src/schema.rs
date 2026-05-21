use anyhow::Result;
use schemars::schema_for;
use serde_json::{json, Map, Value};
use crate::syscall::Syscall;

/// Build a constraint_schema for the plan frame, restricted to `allowed_methods`.
/// Output shape matches 439's `schema_for_methods` — a JSON Schema with a `oneOf`
/// containing one branch per method.
pub fn schema_for_methods(allowed_methods: &[&str]) -> Result<Value> {
    let full = serde_json::to_value(schema_for!(Syscall))?;
    let mut branches = vec![];
    if let Some(one_of) = full.pointer("/oneOf").and_then(|v| v.as_array()) {
        for branch in one_of {
            let method_const = branch.pointer("/properties/method/const")
                .or_else(|| branch.pointer("/properties/method/enum/0"))
                .and_then(|v| v.as_str());
            if let Some(m) = method_const {
                if allowed_methods.contains(&m) {
                    let mut normalized = branch.clone();
                    normalize_provider_schema(&mut normalized);
                    branches.push(normalized);
                }
            }
        }
    }
    Ok(json!({ "oneOf": branches }))
}

/// Recursively normalize a JSON Schema fragment so it's accepted by Anthropic,
/// OpenAI, etc. Mirrors the relevant subset of 439's normalize_provider_schema:
/// strip noise keys, convert `const` to `enum`, default `additionalProperties: false`
/// on objects that declare `properties`.
fn normalize_provider_schema(value: &mut Value) {
    match value {
        Value::Object(object) => normalize_provider_object(object),
        Value::Array(items) => {
            for item in items {
                normalize_provider_schema(item);
            }
        }
        _ => {}
    }
}

fn normalize_provider_object(object: &mut Map<String, Value>) {
    object.remove("$schema");
    object.remove("title");
    object.remove("default");
    object.remove("examples");
    object.remove("format");

    if let Some(value) = object.remove("const") {
        object.insert("enum".to_string(), Value::Array(vec![value]));
    }

    if object.contains_key("properties") && !object.contains_key("additionalProperties") {
        object.insert("additionalProperties".to_string(), Value::Bool(false));
    }

    for value in object.values_mut() {
        normalize_provider_schema(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_schema_with_only_allowed_methods() {
        let schema = schema_for_methods(&["sys_reply", "sys_done"]).unwrap();
        let one_of = schema["oneOf"].as_array().unwrap();
        assert_eq!(one_of.len(), 2);
        let methods: Vec<&str> = one_of.iter()
            .filter_map(|b| b.pointer("/properties/method/const").and_then(|v| v.as_str())
                .or_else(|| b.pointer("/properties/method/enum/0").and_then(|v| v.as_str())))
            .collect();
        assert!(methods.contains(&"sys_reply"));
        assert!(methods.contains(&"sys_done"));
    }

    #[test]
    fn normalizer_strips_noise_keys() {
        let schema = schema_for_methods(&["sys_reply"]).unwrap();
        let branch = &schema["oneOf"][0];
        // schemars 0.8 emits `title` on variant branches; the normalizer should strip them.
        assert!(branch.get("title").is_none(),
            "expected `title` stripped from normalized branch, got: {branch}");
        // `$schema` shouldn't appear on inner branches either.
        assert!(branch.get("$schema").is_none());
    }

    #[test]
    fn normalizer_converts_method_const_to_enum() {
        // schemars 0.8 emits the method tag as `const`; Anthropic doesn't accept `const`,
        // so the normalizer converts it to a single-element `enum`.
        let schema = schema_for_methods(&["sys_reply"]).unwrap();
        let method_schema = &schema["oneOf"][0]["properties"]["method"];
        assert!(method_schema.get("const").is_none(),
            "expected `const` converted to `enum`; got: {method_schema}");
        let enum_arr = method_schema["enum"].as_array()
            .expect("method.enum array should exist after normalization");
        assert_eq!(enum_arr.len(), 1);
        assert_eq!(enum_arr[0].as_str(), Some("sys_reply"));
    }

    #[test]
    fn normalizer_adds_additional_properties_false_to_object_with_properties() {
        let schema = schema_for_methods(&["sys_reply"]).unwrap();
        // The branch itself has `properties: { method, params }` — should get the flag.
        let branch = &schema["oneOf"][0];
        assert_eq!(branch["additionalProperties"].as_bool(), Some(false),
            "expected additionalProperties=false on branch with properties");
    }

    #[test]
    fn empty_allowed_methods_returns_empty_one_of() {
        // Document the current behavior. Task 10's plan-frame builder should guard
        // against passing in an empty allow-list; this test exists so a future change
        // that makes empty-allow an error is intentional rather than accidental.
        let schema = schema_for_methods(&[]).unwrap();
        assert_eq!(schema["oneOf"].as_array().unwrap().len(), 0);
    }
}
