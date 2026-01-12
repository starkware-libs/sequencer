use blockifier::state::state_api::{State, StateReader};
use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_api::core::ContractAddress;
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::OsHintResult;
use crate::hints::hint_implementation::execution::utils::set_state_entry;
use crate::hints::types::HintArgs;
use crate::hints::vars::{Ids, Scope};

pub(crate) fn prepare_state_entry_for_revert(ctx: HintArgs<'_>) -> OsHintResult {
    let contract_address: ContractAddress =
        ctx.get_integer(Ids::ContractAddress.into())?.try_into()?;
    set_state_entry(&contract_address, ctx.vm, ctx.exec_scopes, ctx.ids_data, ctx.ap_tracking)?;

    // Insert the contract address into the execution scopes instead of the entire storage.
    // Later, we use this address to revert the state.
    ctx.exec_scopes.insert_value(Scope::ContractAddressForRevert.into(), contract_address);
    Ok(())
}

pub(crate) fn read_storage_key_for_revert<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintArgs<'_>,
) -> OsHintResult {
    let contract_address: &ContractAddress =
        ctx.exec_scopes.get_ref(Scope::ContractAddressForRevert.into())?;
    let storage_key: StorageKey = ctx.get_integer(Ids::StorageKey.into())?.try_into()?;
    let execution_helper = hint_processor.get_mut_current_execution_helper()?;
    let storage_value =
        execution_helper.cached_state.get_storage_at(*contract_address, storage_key)?;
    ctx.insert_value(Ids::PrevValue.into(), storage_value)?;
    Ok(())
}

pub(crate) fn write_storage_key_for_revert<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintArgs<'_>,
) -> OsHintResult {
    let contract_address: &ContractAddress =
        ctx.exec_scopes.get_ref(Scope::ContractAddressForRevert.into())?;
    let storage_key: StorageKey = ctx.get_integer(Ids::StorageKey.into())?.try_into()?;
    let value = ctx.get_integer(Ids::Value.into())?;
    let execution_helper = hint_processor.get_mut_current_execution_helper()?;
    execution_helper.cached_state.set_storage_at(*contract_address, storage_key, value)?;
    Ok(())
}

pub(crate) fn generate_dummy_os_output_segment(mut ctx: HintArgs<'_>) -> OsHintResult {
    let base = ctx.vm.add_memory_segment();
    let segment_data =
        [MaybeRelocatable::from(ctx.vm.add_memory_segment()), MaybeRelocatable::from(Felt::ZERO)];
    ctx.vm.load_data(base, &segment_data)?;
    ctx.insert_value(Ids::Outputs.into(), base)?;
    Ok(())
}
