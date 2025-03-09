use std::collections::HashMap;

use blockifier::state::state_api::{State, StateReader};
use cairo_vm::any_box;
use cairo_vm::hint_processor::builtin_hint_processor::dict_manager::DictManager;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    insert_value_from_var_name,
    insert_value_into_ap,
};

use crate::hint_processor::execution_helper::StateUpdatePointers;
use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;
use crate::hints::vars::{Const, Ids, InnerStateToPointerDict, Scope};

pub(crate) fn enter_scope_with_aliases<S: StateReader>(
    HintArgs { exec_scopes, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    // Note that aliases, execution_helper and os_input do not enter the new scope as they are not
    // needed.
    let dict_manager_str: &str = Scope::DictManager.into();
    let dict_manager: DictManager = exec_scopes.get(dict_manager_str)?;

    let state_update_trees_pointers_str: &str = Scope::StateUpdateTreePointers.into();
    let state_update_trees_pointers: StateUpdatePointers =
        exec_scopes.get(state_update_trees_pointers_str)?;
    let inner_state_to_pointer_str: &str = Scope::InnerStateToPointer.into();
    let inner_state_to_pointer: InnerStateToPointerDict =
        exec_scopes.get(inner_state_to_pointer_str)?;
    let new_scope = HashMap::from([
        (dict_manager_str.to_string(), any_box!(dict_manager)),
        (inner_state_to_pointer_str.to_string(), any_box!(inner_state_to_pointer)),
        (state_update_trees_pointers_str.to_string(), any_box!(state_update_trees_pointers)),
    ]);
    exec_scopes.enter_scope(new_scope);
    Ok(())
}

pub(crate) fn guess_alias_ptr<S: StateReader>(
    HintArgs { vm, ids_data, ap_tracking, exec_scopes, constants, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let inner_state_to_pointer_str = Scope::InnerStateToPointer.into();
    let mut inner_state_to_pointer: InnerStateToPointerDict =
        exec_scopes.get(inner_state_to_pointer_str)?;
    let aliases_contract_address = Const::get_alias_contract_address(constants)?;
    let aliases_pointer = *inner_state_to_pointer
        .entry(aliases_contract_address)
        .or_insert_with(|| vm.segments.add());

    Ok(insert_value_from_var_name(
        Ids::AliasesEntry.into(),
        aliases_pointer,
        vm,
        ids_data,
        ap_tracking,
    )?)
}

pub(crate) fn update_alias_ptr<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn key_lt_min_alias_alloc_value<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn assert_key_big_enough_for_alias<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn read_alias_from_key<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn write_next_alias_from_key<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn read_alias_counter<S: StateReader>(
    HintArgs { hint_processor, vm, constants, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let aliases_contract_address = Const::get_alias_contract_address(constants)?;
    let alias_counter_storage_key = Const::get_alias_counter_storage_key(constants)?;
    let alias_counter = hint_processor
        .execution_helper
        .cached_state
        .get_storage_at(aliases_contract_address, alias_counter_storage_key)?;
    Ok(insert_value_into_ap(vm, alias_counter)?)
}

pub(crate) fn initialize_alias_counter<S: StateReader>(
    HintArgs { hint_processor, constants, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let aliases_contract_address = Const::get_alias_contract_address(constants)?;
    let alias_counter_storage_key = Const::get_alias_counter_storage_key(constants)?;
    let initial_available_alias = *Const::InitialAvailableAlias.fetch(constants)?;
    Ok(hint_processor.execution_helper.cached_state.set_storage_at(
        aliases_contract_address,
        alias_counter_storage_key,
        initial_available_alias,
    )?)
}

pub(crate) fn update_alias_counter<S: StateReader>(
    HintArgs { hint_processor, constants, ids_data, ap_tracking, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let aliases_contract_address = Const::get_alias_contract_address(constants)?;
    let alias_counter_storage_key = Const::get_alias_counter_storage_key(constants)?;
    let next_available_alias =
        get_integer_from_var_name(Ids::NextAvailableAlias.into(), vm, ids_data, ap_tracking)?;
    Ok(hint_processor.execution_helper.cached_state.set_storage_at(
        aliases_contract_address,
        alias_counter_storage_key,
        next_available_alias,
    )?)
}

pub(crate) fn contract_address_le_max_for_compression<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn compute_commitments_on_finalized_state_with_aliases<S: StateReader>(
    HintArgs { hint_processor, exec_scopes, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    // TODO(Nimrod): Consider moving this hint to `state.rs`.
    // TODO(Nimrod): Try to avoid this clone.
    exec_scopes.insert_value(
        Scope::CommitmentInfoByAddress.into(),
        hint_processor.execution_helper.os_input.address_to_storage_commitment_info.clone(),
    );

    Ok(())
}

pub(crate) fn guess_inner_state_contract_address_ptr<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn update_inner_state_contract_address_ptr<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn guess_state_ptr<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn update_state_ptr<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn guess_classes_ptr<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn update_classes_ptr<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}
