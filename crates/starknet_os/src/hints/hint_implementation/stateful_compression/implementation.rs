use std::collections::HashMap;

use blockifier::state::state_api::{State, StateReader};
use cairo_vm::any_box;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    get_ptr_from_var_name,
    insert_value_from_var_name,
    insert_value_into_ap,
};
use starknet_api::core::ContractAddress;
use starknet_types_core::felt::Felt;

use crate::hint_processor::common_hint_processor::CommonHintProcessor;
use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hint_processor::state_update_pointers::{
    get_contract_state_entry_and_storage_ptr,
    StateEntryPtr,
    StoragePtr,
};
use crate::hints::enum_definition::{AllHints, OsHint};
use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::nondet_offsets::insert_nondet_hint_value;
use crate::hints::types::HintArgs;
use crate::hints::vars::{CairoStruct, Const, Ids, Scope};
use crate::vm_utils::get_address_of_nested_fields;

pub(crate) fn enter_scope_with_aliases(HintArgs { exec_scopes, .. }: HintArgs<'_>) -> OsHintResult {
    // Note that aliases, execution_helper, state_update_pointers and block_input do not enter the
    // new scope as they are not needed.
    let dict_manager = exec_scopes.get_dict_manager()?;
    let new_scope = HashMap::from([(Scope::DictManager.into(), any_box!(dict_manager))]);
    exec_scopes.enter_scope(new_scope);
    Ok(())
}

pub(crate) fn get_class_hash_and_compiled_class_hash_v2<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { ids_data, vm, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let (class_hash, expected_casm_hash_v2) = hint_processor
        .get_mut_current_execution_helper()?
        .class_hashes_to_migrate_iterator
        .next()
        .expect("Class hashes iterator should not be empty");
    insert_value_from_var_name(Ids::ClassHash.into(), class_hash.0, vm, ids_data, ap_tracking)?;
    insert_value_from_var_name(
        Ids::ExpectedCasmHashV2.into(),
        expected_casm_hash_v2.0,
        vm,
        ids_data,
        ap_tracking,
    )?;
    Ok(())
}

pub(crate) fn key_lt_min_alias_alloc_value(
    HintArgs { ids_data, ap_tracking, vm, constants, .. }: HintArgs<'_>,
) -> OsHintResult {
    let key = get_integer_from_var_name(Ids::Key.into(), vm, ids_data, ap_tracking)?;
    let min_value_for_alias_alloc = *Const::MinValueForAliasAlloc.fetch(constants)?;
    Ok(insert_value_into_ap(vm, Felt::from(key < min_value_for_alias_alloc))?)
}

pub(crate) fn assert_key_big_enough_for_alias(
    HintArgs { ids_data, ap_tracking, vm, constants, .. }: HintArgs<'_>,
) -> OsHintResult {
    let key = get_integer_from_var_name(Ids::Key.into(), vm, ids_data, ap_tracking)?;
    let min_value_for_alias_alloc = *Const::MinValueForAliasAlloc.fetch(constants)?;
    if key < min_value_for_alias_alloc {
        Err(OsHintError::AssertionFailed {
            message: format!("Key {key} is too small for alias allocation."),
        })
    } else {
        Ok(())
    }
}

pub(crate) fn read_alias_from_key<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { ids_data, ap_tracking, vm, constants, .. }: HintArgs<'_>,
) -> OsHintResult {
    let key = get_integer_from_var_name(Ids::Key.into(), vm, ids_data, ap_tracking)?;
    let execution_helper = hint_processor.get_current_execution_helper()?;
    let aliases_contract_address = Const::get_alias_contract_address(constants)?;
    let alias =
        execution_helper.cached_state.get_storage_at(aliases_contract_address, key.try_into()?)?;
    insert_nondet_hint_value(vm, AllHints::OsHint(OsHint::ReadAliasFromKey), alias)
}

pub(crate) fn write_next_alias_from_key<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { ids_data, ap_tracking, vm, constants, .. }: HintArgs<'_>,
) -> OsHintResult {
    let key = get_integer_from_var_name(Ids::Key.into(), vm, ids_data, ap_tracking)?;
    let next_available_alias =
        get_integer_from_var_name(Ids::NextAvailableAlias.into(), vm, ids_data, ap_tracking)?;
    let execution_helper = hint_processor.get_mut_current_execution_helper()?;
    let aliases_contract_address = Const::get_alias_contract_address(constants)?;
    Ok(execution_helper.cached_state.set_storage_at(
        aliases_contract_address,
        key.try_into()?,
        next_available_alias,
    )?)
}

pub(crate) fn read_alias_counter<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, constants, .. }: HintArgs<'_>,
) -> OsHintResult {
    let aliases_contract_address = Const::get_alias_contract_address(constants)?;
    let alias_counter_storage_key = Const::get_alias_counter_storage_key(constants)?;
    let alias_counter = hint_processor
        .get_current_execution_helper()?
        .cached_state
        .get_storage_at(aliases_contract_address, alias_counter_storage_key)?;
    Ok(insert_value_into_ap(vm, alias_counter)?)
}

pub(crate) fn initialize_alias_counter<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { constants, .. }: HintArgs<'_>,
) -> OsHintResult {
    let aliases_contract_address = Const::get_alias_contract_address(constants)?;
    let alias_counter_storage_key = Const::get_alias_counter_storage_key(constants)?;
    let initial_available_alias = *Const::InitialAvailableAlias.fetch(constants)?;
    Ok(hint_processor.get_mut_current_execution_helper()?.cached_state.set_storage_at(
        aliases_contract_address,
        alias_counter_storage_key,
        initial_available_alias,
    )?)
}

pub(crate) fn update_alias_counter<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { constants, ids_data, ap_tracking, vm, .. }: HintArgs<'_>,
) -> OsHintResult {
    let aliases_contract_address = Const::get_alias_contract_address(constants)?;
    let alias_counter_storage_key = Const::get_alias_counter_storage_key(constants)?;
    let next_available_alias =
        get_integer_from_var_name(Ids::NextAvailableAlias.into(), vm, ids_data, ap_tracking)?;
    Ok(hint_processor.get_mut_current_execution_helper()?.cached_state.set_storage_at(
        aliases_contract_address,
        alias_counter_storage_key,
        next_available_alias,
    )?)
}

pub(crate) fn contract_address_le_max_for_compression(
    HintArgs { constants, vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let contract_address =
        get_integer_from_var_name(Ids::ContractAddress.into(), vm, ids_data, ap_tracking)?;
    let max_contract_address = *Const::MaxNonCompressedContractAddress.fetch(constants)?;
    Ok(insert_value_into_ap(vm, Felt::from(contract_address <= max_contract_address))?)
}

pub(crate) fn guess_contract_addr_storage_ptr<'program, CHP: CommonHintProcessor<'program>>(
    hint_processor: &mut CHP,
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let key_address = get_address_of_nested_fields(
        ids_data,
        Ids::StateChanges,
        CairoStruct::DictAccessPtr,
        vm,
        ap_tracking,
        &["key"],
        hint_processor.get_program(),
    )?;
    let contract_address = ContractAddress(vm.get_integer(key_address)?.into_owned().try_into()?);
    let (state_entry, storage_ptr) = get_contract_state_entry_and_storage_ptr(
        hint_processor.get_mut_state_update_pointers(),
        vm,
        contract_address,
    );
    insert_value_from_var_name(
        Ids::SquashedPrevState.into(),
        state_entry.0,
        vm,
        ids_data,
        ap_tracking,
    )?;
    insert_value_from_var_name(
        Ids::SquashedStoragePtr.into(),
        storage_ptr.0,
        vm,
        ids_data,
        ap_tracking,
    )?;

    Ok(())
}

pub(crate) fn update_contract_addr_to_storage_ptr<'program, CHP: CommonHintProcessor<'program>>(
    hint_processor: &mut CHP,
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let program = hint_processor.get_program();
    if let Some(state_update_pointers) = hint_processor.get_mut_state_update_pointers() {
        let key_address = get_address_of_nested_fields(
            ids_data,
            Ids::StateChanges,
            CairoStruct::DictAccessPtr,
            vm,
            ap_tracking,
            &["key"],
            program,
        )?;
        let contract_address =
            ContractAddress(vm.get_integer(key_address)?.into_owned().try_into()?);
        let squashed_new_state =
            get_ptr_from_var_name(Ids::SquashedNewState.into(), vm, ids_data, ap_tracking)?;
        let squashed_storage_ptr_end =
            get_ptr_from_var_name(Ids::SquashedStoragePtrEnd.into(), vm, ids_data, ap_tracking)?;

        state_update_pointers.set_contract_state_entry_and_storage_ptr(
            contract_address,
            StateEntryPtr(squashed_new_state),
            StoragePtr(squashed_storage_ptr_end),
        );
    }

    Ok(())
}

pub(crate) fn guess_aliases_contract_storage_ptr<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, constants, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let aliases_contract_address = Const::get_alias_contract_address(constants)?;
    let (state_entry_ptr, storage_ptr) = get_contract_state_entry_and_storage_ptr(
        &mut hint_processor.state_update_pointers,
        vm,
        aliases_contract_address,
    );
    insert_value_from_var_name(
        Ids::PrevAliasesStateEntry.into(),
        state_entry_ptr.0,
        vm,
        ids_data,
        ap_tracking,
    )?;
    insert_value_from_var_name(
        Ids::SquashedAliasesStorageStart.into(),
        storage_ptr.0,
        vm,
        ids_data,
        ap_tracking,
    )?;
    Ok(())
}

pub(crate) fn update_aliases_contract_to_storage_ptr<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, constants, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    if let Some(state_update_pointers) = &mut hint_processor.state_update_pointers {
        let aliases_contract_address = Const::get_alias_contract_address(constants)?;
        let aliases_state_entry_ptr =
            get_ptr_from_var_name(Ids::NewAliasesStateEntry.into(), vm, ids_data, ap_tracking)?;
        let aliases_storage_ptr = get_ptr_from_var_name(
            Ids::SquashedAliasesStorageEnd.into(),
            vm,
            ids_data,
            ap_tracking,
        )?;
        state_update_pointers.set_contract_state_entry_and_storage_ptr(
            aliases_contract_address,
            StateEntryPtr(aliases_state_entry_ptr),
            StoragePtr(aliases_storage_ptr),
        );
    }
    Ok(())
}
