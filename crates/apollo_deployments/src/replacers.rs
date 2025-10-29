use apollo_infra_utils::template::Template;
use serde_json::Value;

const REPLACER_FORMAT: &str = "$$$_{}_$$$";

/// Given a flattened JSON object, overwrite each `key`'s value with `format_key(key)`.
pub(crate) fn insert_replacer_annotations(mut json: Value) -> Value {
    let map = json.as_object_mut().expect("Should be a JSON object");

    // Collect keys to avoid mutable borrow issues while iterating.
    let keys: Vec<String> = map.keys().cloned().collect();
    for key in keys {
        map.insert(key.clone(), Value::String(format_key(key)));
    }

    json
}

fn format_key(key: String) -> String {
    Template::new(REPLACER_FORMAT).format(&[&key]).to_uppercase().replace('.', "-").replace('#', "")
}
