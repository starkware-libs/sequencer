use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_into_ap;
use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_types_core::felt::Felt;

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
    HintArgs { vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let base = vm.add_memory_segment();
    let segment_data =
        [MaybeRelocatable::from(vm.add_memory_segment()), MaybeRelocatable::from(Felt::ZERO)];
    vm.load_data(base, &segment_data)?;
    insert_value_into_ap(vm, base)?;
    Ok(())
}
