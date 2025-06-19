use blockifier::state::state_api::StateReader;

use crate::hint_processor::aggregator_hint_processor::AggregatorHintProcessor;
use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;

/// Test hint for debugging. Implement this hint however you like, but should not be merged with
/// an actual implementation.
/// As long as the hint string starts with TEST_HINT_PREFIX (possibly preceded by whitespace),
/// it will be recognized as the test hint and this implementation will be called. For example:
///
/// %{
///     # TEST HINT 7
///     print("Debug hint 7")
/// %}
///
/// The original hint string is passed as the first argument to allow injecting multiple test
/// hints; the implementation can differ depending on the hint string. With the example above, an
/// example implementation could look like:
/// ```ignore
/// pub(crate) fn test_hint<S: StateReader>(
///     hint_str: &str,
///     hint_processor: &mut SnosHintProcessor<'_, S>,
///     HintArgs { .. }: HintArgs<'_>,
/// ) -> OsHintResult {
///     let hint_case = hint_str.trim_start().strip_prefix(TEST_HINT_PREFIX).unwrap().trim_start();
///     match hint_case[0] {
///         '7' => println!("Debug hint 7"),
///         other => panic!("Unknown test hint case {other}."),
///     }
///     Ok(())
/// }
/// ```
pub(crate) fn test_hint<S: StateReader>(
    _hint_str: &str,
    _hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    Ok(())
}

/// Same as [test_hint], but for the aggregator program.
pub(crate) fn test_aggregator_hint(
    _hint_str: &str,
    _hint_processor: &mut AggregatorHintProcessor<'_>,
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    Ok(())
}
