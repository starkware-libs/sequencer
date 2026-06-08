use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::Value;

use crate::errors::ReexecutionResult;

// TODO(Aner): import the following functions instead, to reduce code duplication.
pub(crate) fn hashmap_from_raw<
    K: for<'de> Deserialize<'de> + Eq + std::hash::Hash,
    V: for<'de> Deserialize<'de>,
>(
    raw_object: &Value,
    vec_str: &str,
    key_str: &str,
    value_str: &str,
) -> ReexecutionResult<IndexMap<K, V>> {
    Ok(vec_to_hashmap::<K, V>(
        serde_json::from_value(raw_object[vec_str].clone())?,
        key_str,
        value_str,
    ))
}

pub(crate) fn nested_hashmap_from_raw<
    K: for<'de> Deserialize<'de> + Eq + std::hash::Hash,
    VK: for<'de> Deserialize<'de> + Eq + std::hash::Hash,
    VV: for<'de> Deserialize<'de>,
>(
    raw_object: &Value,
    vec_str: &str,
    key_str: &str,
    value_str: &str,
    inner_key_str: &str,
    inner_value_str: &str,
) -> ReexecutionResult<IndexMap<K, IndexMap<VK, VV>>> {
    Ok(vec_to_nested_hashmap::<K, VK, VV>(
        serde_json::from_value(raw_object[vec_str].clone())?,
        key_str,
        value_str,
        inner_key_str,
        inner_value_str,
    ))
}

pub(crate) fn vec_to_hashmap<
    K: for<'de> Deserialize<'de> + Eq + std::hash::Hash,
    V: for<'de> Deserialize<'de>,
>(
    vec: Vec<Value>,
    key_str: &str,
    value_str: &str,
) -> IndexMap<K, V> {
    vec.iter()
        .map(|element| {
            (
                serde_json::from_value(element[key_str].clone())
                    .expect("Key string doesn't match expected."),
                serde_json::from_value(element[value_str].clone())
                    .expect("Value string doesn't match expected."),
            )
        })
        .collect()
}

pub(crate) fn vec_to_nested_hashmap<
    K: for<'de> Deserialize<'de> + Eq + std::hash::Hash,
    VK: for<'de> Deserialize<'de> + Eq + std::hash::Hash,
    VV: for<'de> Deserialize<'de>,
>(
    vec: Vec<Value>,
    key_str: &str,
    value_str: &str,
    inner_key_str: &str,
    inner_value_str: &str,
) -> IndexMap<K, IndexMap<VK, VV>> {
    vec.iter()
        .map(|element| {
            (
                serde_json::from_value(element[key_str].clone()).expect("Couldn't deserialize key"),
                vec_to_hashmap(
                    serde_json::from_value(element[value_str].clone())
                        .expect("Couldn't deserialize value"),
                    inner_key_str,
                    inner_value_str,
                ),
            )
        })
        .collect()
}
