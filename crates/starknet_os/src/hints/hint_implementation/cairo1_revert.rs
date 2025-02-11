use crate::hints::error::HintResult;
use crate::hints::types::HintArgs;

pub fn prepare_state_entry_for_revert(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}

pub fn read_storage_key_for_revert(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}

pub fn write_storage_key_for_revert(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}

pub fn generate_dummy_os_output_segment(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}
