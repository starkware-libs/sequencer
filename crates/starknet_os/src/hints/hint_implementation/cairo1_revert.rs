use crate::hints::error::HintResult;
use crate::hints::types::HintArgs;

pub(crate) fn prepare_state_entry_for_revert(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}

pub(crate) fn read_storage_key_for_revert(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}

pub(crate) fn write_storage_key_for_revert(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}

pub(crate) fn generate_dummy_os_output_segment(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}
