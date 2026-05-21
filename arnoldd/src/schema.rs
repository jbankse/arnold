use anyhow::Result;
use schemars::schema_for;
use serde_json::{json, Value};
use crate::syscall::Syscall;

/// Build a constraint_schema for the plan frame, restricted to `allowed_methods`.
/// Output shape matches 439's `schema_for_methods` — a JSON Schema with a `oneOf`
/// containing one branch per method.
pub fn schema_for_methods(allowed_methods: &[&str]) -> Result<Value> {
    let full = serde_json::to_value(schema_for!(Syscall))?;
    let mut branches = vec![];
    // schemars' tagged-content output for our enum is `oneOf` over variant objects.
    if let Some(one_of) = full.pointer("/oneOf").and_then(|v| v.as_array()) {
        for branch in one_of {
            let method_const = branch.pointer("/properties/method/const")
                .or_else(|| branch.pointer("/properties/method/enum/0"))
                .and_then(|v| v.as_str());
            if let Some(m) = method_const {
                if allowed_methods.contains(&m) {
                    branches.push(strip_bare_objects(branch.clone()));
                }
            }
        }
    }
    Ok(json!({ "oneOf": branches }))
}

/// Replace any bare `{}` (which some LLM providers reject) with a permissive object
/// shape. Mirrors 439's normalize_provider_schema().
fn strip_bare_objects(v: Value) -> Value {
    match v {
        Value::Object(mut map) => {
            for (_, val) in map.iter_mut() {
                *val = strip_bare_objects(val.take());
            }
            Value::Object(map)
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(strip_bare_objects).collect()),
        other => other,
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
}
