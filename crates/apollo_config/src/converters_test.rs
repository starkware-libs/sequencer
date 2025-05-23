use serde::Deserialize;

use crate::converters::deserialize_optional_vec;

#[derive(Deserialize)]
struct TestConfig {
    #[serde(deserialize_with = "deserialize_optional_vec")]
    optional_vec: Option<Vec<i32>>,
}

#[test]
fn test_deserialize_optional_vec_empty_value_string() {
    let cfg_string = r#"{ "optional_vec": "" }"#;
    let cfg: TestConfig = serde_json::from_str(cfg_string).unwrap();
    assert!(cfg.optional_vec.is_none());
}

#[test]
fn test_deserialize_optional_vec_single_value() {
    let cfg_string = r#"{ "optional_vec": "-15" }"#;
    let cfg: TestConfig = serde_json::from_str(cfg_string).unwrap();
    assert_eq!(cfg.optional_vec.expect("Desrialize returned None"), vec![-15]);
}

#[test]
fn test_deserialize_optional_vec_mutliple_values() {
    let cfg_string = r#"{ "optional_vec": "6,-15" }"#;
    let cfg: TestConfig = serde_json::from_str(cfg_string).unwrap();
    assert_eq!(cfg.optional_vec.expect("Desrialize returned None"), vec![6, -15]);
}

#[test]
fn test_deserialize_optional_vec_ignores_empty_values() {
    let cfg_string = r#"{ "optional_vec": "6,,-15" }"#;
    let cfg: TestConfig = serde_json::from_str(cfg_string).unwrap();
    assert_eq!(cfg.optional_vec.expect("Desrialize returned None"), vec![6, -15]);
}
