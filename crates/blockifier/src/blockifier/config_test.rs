use rstest::rstest;
use serde_json;
use starknet_api::class_hash;

use crate::blockifier::config::NativeClassesWhitelist;

#[rstest]
#[case::all(NativeClassesWhitelist::All, "\"All\"")]
#[case::limited(
    NativeClassesWhitelist::Limited(vec![class_hash!("0x1234"), class_hash!("0x5678")]),
    "\"[\\\"0x1234\\\",\\\"0x5678\\\"]\"",
)]
fn test_cairo_native_whitelist_serializes_and_back(
    #[case] value: NativeClassesWhitelist,
    #[case] expected_json: &str,
) {
    let serialized = serde_json::to_string(&value).expect("serialize");
    assert_eq!(serialized, expected_json);
    let deserialized: NativeClassesWhitelist =
        serde_json::from_str(expected_json).expect("deserialize");
    assert_eq!(deserialized, value);
}
