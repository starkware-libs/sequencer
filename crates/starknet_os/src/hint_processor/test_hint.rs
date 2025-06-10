use blockifier::state::state_api::StateReader;

use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;

/// Test hint for debugging. Implement this hint however you like, but should not be merged with
/// an actual implementation.
/// As long as the hint string starts with TEST_HINT_PREFIX (possibly preceded by whitespace),
/// it will be recognized as the test hint and this implementation will be called. For example:
/// ```
/// %{
///     # TEST HINT 7
///     print("Debug hint 7")
/// %}
/// ```
/// The original hint string is passed as the first argument to allow injecting multiple test
/// hints; the implementation can differ depending on the hint string. With the example above, an
/// example implementation could look like:
/// ```
/// let hint_case = hint_str.trim_start().strip_prefix(TEST_HINT_PREFIX).unwrap().trim_start();
/// match hint_case[0] {
///     '7' => println!("Debug hint 7"),
///     other => panic!("Unknown test hint case {other}."),
/// }
/// ```
#[allow(clippy::result_large_err)]
pub(crate) fn test_hint<S: StateReader>(
    _hint_str: &str,
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    Ok(())
}
