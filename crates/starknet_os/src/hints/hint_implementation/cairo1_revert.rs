use blockifier::state::state_api::StateReader;

use crate::hints::error::HintResult;
use crate::hints::types::HintArgs;

pub(crate) fn prepare_state_entry_for_revert<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn read_storage_key_for_revert<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn write_storage_key_for_revert<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn generate_dummy_os_output_segment<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}
