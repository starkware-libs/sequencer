use apollo_infra_utils::template::Template;
use serde_json::Value;
const REPLACER_FORMAT: &str = "$$$_{}_$$$";

pub(crate) fn insert_replacer_annotations<F>(mut json: Value, pred: F) -> Value
where
    F: Fn(&str, &Value) -> bool,
{
    let map = json.as_object_mut().expect("Should be a JSON object");

    // Collect keys to avoid mutable borrow issues while iterating.
    let keys: Vec<String> = map.keys().cloned().collect();
    for key in keys {
        let should_replace = {
            // Evaluate predicate on current value
            let value = map.get(&key).expect("Key must exist");
            pred(&key, value)
        };

        if should_replace {
            map.insert(key.clone(), Value::String(format_key(key.clone())));
        }
    }

    json
}

fn format_key(key: String) -> String {
    Template::new(REPLACER_FORMAT).format(&[&key]).to_uppercase().replace('.', "-").replace('#', "")
}
