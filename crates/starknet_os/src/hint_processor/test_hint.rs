use blockifier::state::state_api::StateReader;

use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;

/// Test hint for debugging. Implement this hint however you like, but should not be merged with
/// an actual implementation.
/// As long as the hint string starts with TEST_HINT_PREFIX (possibly preceded by whitespace),
/// it will be recognized as the test hint and this implementation will be called.
/// The original hint string is passed as the first argument to allow injecting multiple test
/// hints; the implementation can differ depending on the hint string.
#[allow(clippy::result_large_err)]
pub(crate) fn test_hint<S: StateReader>(
    _hint_str: &str,
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    Ok(())
}
