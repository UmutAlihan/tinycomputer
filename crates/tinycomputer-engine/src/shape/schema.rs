//! The JSON Schema subset a task's `output.schema` may use, and the check a
//! result must pass against it.
//!
//! A full JSON Schema validator is a large dependency for what a task result
//! needs: the shape of objects, arrays, and scalars. The subset is small and
//! checked exhaustively, and a schema reaching outside it is refused up front
//! rather than silently half-checked.

use serde_json::{Map, Value};

/// The keywords a schema may use.
pub(crate) const KEYWORDS: &[&str] = &[
    "type",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "enum",
    "minItems",
    "maxItems",
    "description",
    "title",
];

/// The type names `type` may hold.
const TYPES: &[&str] = &[
    "object", "array", "string", "number", "integer", "boolean", "null",
];

/// Why `schema` is outside the subset, or `Ok` when it is inside.
///
/// # Errors
///
/// The first problem found, naming where in the schema it is.
pub(crate) fn supported(schema: &Value) -> Result<(), String> {
    // The result is always a JSON object: a model asked for JSON answers
    // with one, and a caller reads named fields.
    if let Some(kind) = schema.get("type")
        && kind != "object"
    {
        return Err("the schema's top-level `type` must be \"object\"".to_owned());
    }
    supported_at(schema, "the schema")
}

fn supported_at(schema: &Value, at: &str) -> Result<(), String> {
    let Value::Object(schema) = schema else {
        return Err(format!("{at} must be an object"));
    };
    if let Some(keyword) = schema.keys().find(|key| !KEYWORDS.contains(&key.as_str())) {
        return Err(format!(
            "{at} uses `{keyword}`; the supported keywords are {}",
            KEYWORDS.join(", ")
        ));
    }
    if let Some(kind) = schema.get("type") {
        let names = match kind {
            Value::String(name) => vec![name.as_str()],
            Value::Array(names) => names.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        if names.is_empty() || names.iter().any(|name| !TYPES.contains(name)) {
            return Err(format!(
                "{at} has a `type` that is not one of {}, or a list of them",
                TYPES.join(", ")
            ));
        }
    }
    if let Some(properties) = schema.get("properties") {
        let Value::Object(properties) = properties else {
            return Err(format!("{at}: `properties` must be an object"));
        };
        for (name, property) in properties {
            supported_at(property, &format!("{at}.{name}"))?;
        }
    }
    if let Some(required) = schema.get("required")
        && !required
            .as_array()
            .is_some_and(|names| names.iter().all(Value::is_string))
    {
        return Err(format!("{at}: `required` must be a list of names"));
    }
    if let Some(extra) = schema.get("additionalProperties")
        && !extra.is_boolean()
    {
        return Err(format!(
            "{at}: `additionalProperties` must be true or false"
        ));
    }
    if let Some(items) = schema.get("items") {
        supported_at(items, &format!("{at}[]"))?;
    }
    if let Some(choices) = schema.get("enum")
        && !choices.is_array()
    {
        return Err(format!("{at}: `enum` must be a list"));
    }
    for bound in ["minItems", "maxItems"] {
        if let Some(value) = schema.get(bound)
            && value.as_u64().is_none()
        {
            return Err(format!("{at}: `{bound}` must be a whole number"));
        }
    }
    Ok(())
}

/// Every way `value` breaks `schema`, by where in the value; empty when it
/// satisfies it. `schema` is assumed [`supported`].
#[must_use]
pub(crate) fn violations(value: &Value, schema: &Value) -> Vec<String> {
    let mut found = Vec::new();
    check(value, schema, "the result", &mut found);
    found
}

fn check(value: &Value, schema: &Value, at: &str, found: &mut Vec<String>) {
    let Value::Object(schema) = schema else {
        return;
    };
    if let Some(kind) = schema.get("type")
        && !type_matches(value, kind)
    {
        found.push(format!(
            "{at} must be of type {kind}, not {}",
            type_of(value)
        ));
        return;
    }
    if let Some(Value::Array(choices)) = schema.get("enum")
        && !choices.contains(value)
    {
        found.push(format!(
            "{at} must be one of {}",
            Value::Array(choices.clone())
        ));
    }
    match value {
        Value::Object(fields) => check_object(fields, schema, at, found),
        Value::Array(items) => {
            let count = items.len() as u64;
            if let Some(least) = schema.get("minItems").and_then(Value::as_u64)
                && count < least
            {
                found.push(format!(
                    "{at} must hold at least {least} items, not {count}"
                ));
            }
            if let Some(most) = schema.get("maxItems").and_then(Value::as_u64)
                && count > most
            {
                found.push(format!("{at} must hold at most {most} items, not {count}"));
            }
            if let Some(item) = schema.get("items") {
                for (index, value) in items.iter().enumerate() {
                    check(value, item, &format!("{at}[{index}]"), found);
                }
            }
        }
        _ => {}
    }
}

fn check_object(
    fields: &Map<String, Value>,
    schema: &Map<String, Value>,
    at: &str,
    found: &mut Vec<String>,
) {
    let properties = schema.get("properties").and_then(Value::as_object);
    if let Some(Value::Array(required)) = schema.get("required") {
        for name in required.iter().filter_map(Value::as_str) {
            if !fields.contains_key(name) {
                found.push(format!("{at} is missing `{name}`"));
            }
        }
    }
    for (name, value) in fields {
        match properties.and_then(|properties| properties.get(name)) {
            Some(property) => check(value, property, &format!("{at}.{name}"), found),
            None if schema.get("additionalProperties") == Some(&Value::Bool(false)) => {
                found.push(format!(
                    "{at} has `{name}`, which the schema does not allow"
                ));
            }
            None => {}
        }
    }
}

fn type_matches(value: &Value, kind: &Value) -> bool {
    let is = |name: &str| match name {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => value.is_i64() || value.is_u64(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => false,
    };
    match kind {
        Value::String(name) => is(name),
        Value::Array(names) => names.iter().filter_map(Value::as_str).any(is),
        _ => false,
    }
}

fn type_of(value: &Value) -> &'static str {
    match value {
        Value::Object(_) => "object",
        Value::Array(_) => "array",
        Value::String(_) => "string",
        Value::Number(_) => "number",
        Value::Bool(_) => "boolean",
        Value::Null => "null",
    }
}
