use rstest::rstest;
use serde_json;
use starknet_api::class_hash;
use validator::Validate;

use crate::blockifier::config::{CairoNativeRunConfig, NativeClassesWhitelist};

#[rstest]
#[case::all(NativeClassesWhitelist::All)]
#[case::limited(
    NativeClassesWhitelist::Limited(vec![class_hash!("0x1234"), class_hash!("0x5678")])
)]
fn test_native_classes_whitelist_serializes_and_back(#[case] value: NativeClassesWhitelist) {
    let serialized = serde_json::to_string(&value).unwrap();
    let deserialized: NativeClassesWhitelist = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized, value);
}

#[rstest]
#[case(true, true, true)]
#[case(true, false, true)]
#[case(false, true, false)]
#[case(false, false, true)]
fn test_validate_run_cairo_native(
    #[case] run_cairo_native: bool,
    #[case] wait_on_native_compilation: bool,
    #[case] should_be_valid: bool,
) {
    let config =
        CairoNativeRunConfig { run_cairo_native, wait_on_native_compilation, ..Default::default() };
    let result = config.validate();
    assert_eq!(result.is_ok(), should_be_valid, "unexpected validation result {result:?}");
}
