use apollo_infra_utils::template::Template;
use serde_json::Value;

const REPLACER_FORMAT: &str = "$$$_REPLACE_{}$$$";

/// Given a flattened JSON object, overwrite each `key`'s value with `format_key(key)`.
pub(crate) fn insert_replacer_annotations(mut json: Value) -> Value {
    // Assert it’s an object (serde_json::Value::Object)
    assert!(json.is_object(), "expected a JSON object, got: {}", json);

    // SAFETY: we just asserted it's an object.
    let map = json.as_object_mut().unwrap();

    // Collect keys to avoid mutable borrow issues while iterating.
    let keys: Vec<String> = map.keys().cloned().collect();
    for key in keys {
        map.insert(key.clone(), Value::String(format_key(key)));
    }

    json
}

fn format_key(key: String) -> String {
    Template::new(REPLACER_FORMAT).format(&[&key])
}
