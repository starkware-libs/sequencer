//! Utils for config test.

use assert_json_diff::{assert_json_matches_no_panic, CompareMode, Config};
use serde::Serialize;

/// Compares two JSON values for an exact match without panicking.
/// See [`assert_json_matches_no_panic`]
pub fn assert_json_exact_matches_no_panic<Lhs, Rhs>(lhs: &Lhs, rhs: &Rhs) -> Result<(), String>
where
    Lhs: Serialize,
    Rhs: Serialize,
{
    assert_json_matches_no_panic(lhs, rhs, Config::new(CompareMode::Strict))
}

#[macro_export]
/// Compare two JSON values for an exact match.
///
/// Extends the functionality of [`assert_json_diff::assert_json_eq`] by also adding a customizable
/// error message print.
macro_rules! assert_json_eq {
    ($lhs:expr, $rhs:expr, $error_message:expr $(,)?) => {{
        if let Err(error) =
            $crate::test_utils::assert_json_eq::assert_json_exact_matches_no_panic(&$lhs, &$rhs)
        {
            let printed_error = format!("\n\n{}\n{}\n\n", $error_message, error);
            panic!("{}", printed_error);
        }
    }};

    ($lhs:expr, $rhs:expr $(,)?) => {{
        if let Err(error) = $crate::test_utils::assert_json_exact_matches_no_panic(&$lhs, &$rhs) {
            let printed_error = format!("\n\n{}\n\n", error);
            panic!("{}", printed_error);
        }
    }};
}
