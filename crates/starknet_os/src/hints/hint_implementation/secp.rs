use blockifier::state::state_api::StateReader;

use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;

#[allow(clippy::result_large_err)]
pub(crate) fn is_on_curve<S: StateReader>(HintArgs { .. }: HintArgs<'_, '_, S>) -> OsHintResult {
    todo!()
}

#[allow(clippy::result_large_err)]
pub(crate) fn read_ec_point_from_address<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}
