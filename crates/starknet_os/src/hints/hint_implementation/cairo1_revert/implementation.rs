use blockifier::state::state_api::{State, StateReader};
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    insert_value_into_ap,
};
use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_api::core::ContractAddress;
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::OsHintResult;
use crate::hints::hint_implementation::execution::utils::set_state_entry;
use crate::hints::types::HintArgs;
use crate::hints::vars::{Ids, Scope};

pub(crate) fn prepare_state_entry_for_revert(
    HintArgs { ids_data, ap_tracking, vm, exec_scopes, .. }: HintArgs<'_>,
) -> OsHintResult {
    let contract_address: ContractAddress =
        get_integer_from_var_name(Ids::ContractAddress.into(), vm, ids_data, ap_tracking)?
            .try_into()?;
    set_state_entry(&contract_address, vm, exec_scopes, ids_data, ap_tracking)?;

    // Insert the contract address into the execution scopes instead of the entire storage.
    // Later, we use this address to revert the state.
    exec_scopes.insert_value(Scope::ContractAddressForRevert.into(), contract_address);
    Ok(())
}

pub(crate) fn read_storage_key_for_revert<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { exec_scopes, vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let contract_address: &ContractAddress =
        exec_scopes.get_ref(Scope::ContractAddressForRevert.into())?;
    let storage_key: StorageKey =
        get_integer_from_var_name(Ids::StorageKey.into(), vm, ids_data, ap_tracking)?.try_into()?;
    let execution_helper = hint_processor.get_mut_current_execution_helper()?;
    let storage_value =
        execution_helper.cached_state.get_storage_at(*contract_address, storage_key)?;
    insert_value_into_ap(vm, storage_value)?;
    Ok(())
}

pub(crate) fn write_storage_key_for_revert<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { exec_scopes, vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let contract_address: &ContractAddress =
        exec_scopes.get_ref(Scope::ContractAddressForRevert.into())?;
    let storage_key: StorageKey =
        get_integer_from_var_name(Ids::StorageKey.into(), vm, ids_data, ap_tracking)?.try_into()?;
    let value = get_integer_from_var_name(Ids::Value.into(), vm, ids_data, ap_tracking)?;
    let execution_helper = hint_processor.get_mut_current_execution_helper()?;
    execution_helper.cached_state.set_storage_at(*contract_address, storage_key, value)?;
    Ok(())
}

pub(crate) fn generate_dummy_os_output_segment(HintArgs { vm, .. }: HintArgs<'_>) -> OsHintResult {
    let base = vm.add_memory_segment();
    let segment_data =
        [MaybeRelocatable::from(vm.add_memory_segment()), MaybeRelocatable::from(Felt::ZERO)];
    vm.load_data(base, &segment_data)?;
    insert_value_into_ap(vm, base)?;
    Ok(())
}
