use blockifier::state::state_api::StateReader;

use crate::hints::error::HintResult;
use crate::hints::types::HintArgs;

pub(crate) fn set_preimage_for_state_commitments<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn set_preimage_for_class_commitments<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn set_preimage_for_current_commitment_info<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn load_edge<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn load_bottom<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn decode_node<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn write_split_result<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    todo!()
}
