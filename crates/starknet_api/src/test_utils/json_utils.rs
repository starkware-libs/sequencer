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
        assert_json_eq!(@inner $lhs, $rhs, Some($error_message))
    }};

    ($lhs:expr, $rhs:expr $(,)?) => {{
        assert_json_eq!(@inner $lhs, $rhs, None::<&str>)
    }};

    (@inner $lhs:expr, $rhs:expr, $optional_msg:expr) => {{
        if let Err(error) = $crate::test_utils::json_utils::assert_json_exact_matches_no_panic(
            &$lhs, &$rhs
        ) {
            let printed_error = match $optional_msg {
                Some(msg) => format!("\n\n{}\n{}\n\n", msg, error),
                None => format!("\n\n{}\n\n", error),
            };
            panic!("{}", printed_error);
        }
    }};
}
