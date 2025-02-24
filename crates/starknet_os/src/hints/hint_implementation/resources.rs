use blockifier::state::state_api::StateReader;

use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;

pub(crate) fn remaining_gas_gt_max<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn debug_expected_initial_gas<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn is_sierra_gas_mode<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}
