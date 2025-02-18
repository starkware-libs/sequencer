use blockifier::state::state_api::StateReader;

use crate::hints::error::HintResult;
use crate::hints::types::HintArgs;

pub(crate) fn set_tree_structure<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn set_state_updates_start<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn set_compressed_start<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn set_n_updates_small<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    todo!()
}
