use std::collections::HashMap;

use blockifier::state::state_api::{State, StateReader};
use cairo_vm::any_box;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    get_relocatable_from_var_name,
    insert_value_from_var_name,
    insert_value_into_ap,
};
use starknet_types_core::felt::Felt;

use crate::hint_processor::state_update_pointers::get_contract_state_entry_and_storage_ptr;
use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;
use crate::hints::vars::{Const, Ids, Scope};

pub(crate) fn enter_scope_with_aliases<S: StateReader>(
    HintArgs { exec_scopes, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    // Note that aliases, execution_helper, state_update_pointers and block_input do not enter the
    // new scope as they are not needed.
    let dict_manager = exec_scopes.get_dict_manager()?;
    let new_scope = HashMap::from([(Scope::DictManager.into(), any_box!(dict_manager))]);
    exec_scopes.enter_scope(new_scope);
    Ok(())
}

pub(crate) fn key_lt_min_alias_alloc_value<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn assert_key_big_enough_for_alias<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn read_alias_from_key<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn write_next_alias_from_key<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn read_alias_counter<S: StateReader>(
    HintArgs { hint_processor, vm, constants, .. }: HintArgs<'_, '_, S>,
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
    HintArgs { hint_processor, constants, .. }: HintArgs<'_, '_, S>,
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
    HintArgs { hint_processor, constants, ids_data, ap_tracking, vm, .. }: HintArgs<'_, '_, S>,
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

pub(crate) fn contract_address_le_max_for_compression<S: StateReader>(
    HintArgs { constants, vm, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let contract_address =
        get_integer_from_var_name(Ids::ContractAddress.into(), vm, ids_data, ap_tracking)?;
    let max_contract_address = *Const::MaxNonCompressedContractAddress.fetch(constants)?;
    Ok(insert_value_into_ap(vm, Felt::from(contract_address <= max_contract_address))?)
}

pub(crate) fn guess_contract_addr_storage_ptr<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn update_contract_addr_to_storage_ptr<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn guess_aliases_contract_storage_ptr<S: StateReader>(
    HintArgs { hint_processor, vm, constants, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let aliases_contract_address = Const::get_alias_contract_address(constants)?;
    let (state_entry_ptr, storage_ptr) = get_contract_state_entry_and_storage_ptr(
        &mut hint_processor.state_update_pointers,
        vm,
        aliases_contract_address,
    );
    insert_value_from_var_name(
        Ids::PrevAliasesStateEntry.into(),
        state_entry_ptr,
        vm,
        ids_data,
        ap_tracking,
    )?;
    insert_value_from_var_name(
        Ids::SquashedAliasesStorageStart.into(),
        storage_ptr,
        vm,
        ids_data,
        ap_tracking,
    )?;
    Ok(())
}

pub(crate) fn update_aliases_contract_to_storage_ptr<S: StateReader>(
    HintArgs { hint_processor, vm, constants, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    if let Some(state_update_pointers) = &mut hint_processor.state_update_pointers {
        let aliases_contract_address = Const::get_alias_contract_address(constants)?;
        let aliases_state_entry_ptr = get_relocatable_from_var_name(
            Ids::NewAliasesStateEntry.into(),
            vm,
            ids_data,
            ap_tracking,
        )?;
        let aliases_storage_ptr = get_relocatable_from_var_name(
            Ids::SquashedAliasesStorageEnd.into(),
            vm,
            ids_data,
            ap_tracking,
        )?;
        state_update_pointers.set_contract_state_entry_and_storage_ptr(
            aliases_contract_address,
            aliases_state_entry_ptr,
            aliases_storage_ptr,
        );
    }
    Ok(())
}
