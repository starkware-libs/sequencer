use blockifier::state::state_api::StateReader;

use crate::hints::error::HintResult;
use crate::hints::types::HintArgs;

pub(crate) fn is_on_curve<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn read_ec_point_from_address<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    todo!()
}
