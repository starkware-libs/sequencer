use rstest::rstest;

use crate::legacy_params::{parse_legacy_json_scalar, LegacyJsonScalar};

/// Every message here is the exact live response text for the same input (verified 2026-06-03).
#[rstest]
#[case::letters("zzz", "Expecting value: line 1 column 1 (char 0)")]
#[case::latest("latest", "Expecting value: line 1 column 1 (char 0)")]
#[case::pending("pending", "Expecting value: line 1 column 1 (char 0)")]
#[case::empty("", "Expecting value: line 1 column 1 (char 0)")]
#[case::bare_minus("-", "Expecting value: line 1 column 1 (char 0)")]
#[case::block_hash(
    "0x78b67b11f8c23850041e11fb0f3b39db0bcb2c99d756d5a81321d1b483d79f6",
    "Extra data: line 1 column 2 (char 1)"
)]
#[case::leading_zero("01", "Extra data: line 1 column 2 (char 1)")]
#[case::digits_then_letters("123abc", "Extra data: line 1 column 4 (char 3)")]
fn json_loads_error_messages(#[case] raw_value: &str, #[case] live_message: &str) {
    assert_eq!(parse_legacy_json_scalar(raw_value), Err(live_message.to_string()));
}

#[rstest]
#[case::null("null", LegacyJsonScalar::Null)]
#[case::bool_true("true", LegacyJsonScalar::Bool(true))]
#[case::bool_false("false", LegacyJsonScalar::Bool(false))]
#[case::small_int(
    "7",
    LegacyJsonScalar::Int { python_repr: "7".to_string(), negative: false, value: Some(7) }
)]
#[case::negative_int(
    "-1",
    LegacyJsonScalar::Int { python_repr: "-1".to_string(), negative: true, value: None }
)]
#[case::beyond_u64_int(
    "99999999999999999999999",
    LegacyJsonScalar::Int {
        python_repr: "99999999999999999999999".to_string(),
        negative: false,
        value: None
    }
)]
#[case::float_truncates(
    "1.5",
    LegacyJsonScalar::Float { python_repr: "1.5".to_string(), truncated: 1 }
)]
#[case::negative_float_truncates_toward_zero(
    "-1.5",
    LegacyJsonScalar::Float { python_repr: "-1.5".to_string(), truncated: -1 }
)]
fn json_loads_scalars(#[case] raw_value: &str, #[case] expected_scalar: LegacyJsonScalar) {
    assert_eq!(parse_legacy_json_scalar(raw_value), Ok(expected_scalar));
}
