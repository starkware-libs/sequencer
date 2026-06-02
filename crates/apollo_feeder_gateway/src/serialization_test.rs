use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::serialization::to_python_json;

#[test]
fn spaced_separators_match_python() {
    let value = json!({"a": 1, "b": [1, 2]});
    assert_eq!(to_python_json(&value).unwrap(), r#"{"a": 1, "b": [1, 2]}"#);
}

#[test]
fn empty_containers() {
    assert_eq!(to_python_json(&json!([])).unwrap(), "[]");
    assert_eq!(to_python_json(&json!({})).unwrap(), "{}");
}

#[test]
fn nested_containers() {
    let value = json!({"outer": {"inner": [1, {"k": "v"}]}});
    assert_eq!(to_python_json(&value).unwrap(), r#"{"outer": {"inner": [1, {"k": "v"}]}}"#);
}

#[test]
fn non_ascii_is_escaped_like_python_ensure_ascii() {
    // Python json.dumps("café", ensure_ascii=True) escapes the non-ASCII scalar as é.
    assert_eq!(to_python_json(&json!("café")).unwrap(), "\"caf\\u00e9\"");
    // Code point above the BMP -> UTF-16 surrogate pair (emoji "😀" = U+1F600).
    assert_eq!(to_python_json(&json!("😀")).unwrap(), "\"\\ud83d\\ude00\"");
}

#[test]
fn standard_escapes_still_apply() {
    assert_eq!(to_python_json(&json!("a\"b\\c\n")).unwrap(), r#""a\"b\\c\n""#);
}

/// B4 round-trip lock: optional fields that are `None` must be omitted (never serialized as
/// `null`), and a deserialize -> `to_python_json` round trip must reproduce the original bytes.
#[test]
fn optional_none_fields_are_omitted() {
    #[derive(Serialize, Deserialize)]
    struct Sample {
        present: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        absent: Option<u64>,
    }

    let original = r#"{"present": 7}"#;
    let parsed: Sample = serde_json::from_str(original).unwrap();
    assert_eq!(to_python_json(&parsed).unwrap(), original);
}
