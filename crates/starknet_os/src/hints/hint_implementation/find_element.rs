use blockifier::state::state_api::StateReader;

use crate::hints::error::HintResult;
use crate::hints::types::HintArgs;

pub(crate) fn search_sorted_optimistic<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}
