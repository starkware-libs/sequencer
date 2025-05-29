use blockifier::state::state_api::StateReader;

use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;

#[allow(clippy::result_large_err)]
pub(crate) fn prepare_state_entry_for_revert<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

#[allow(clippy::result_large_err)]
pub(crate) fn read_storage_key_for_revert<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

#[allow(clippy::result_large_err)]
pub(crate) fn write_storage_key_for_revert<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

#[allow(clippy::result_large_err)]
pub(crate) fn generate_dummy_os_output_segment<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}
