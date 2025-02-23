use std::collections::HashMap;

use blockifier::state::state_api::{State, StateReader};
use cairo_vm::any_box;
use cairo_vm::hint_processor::builtin_hint_processor::dict_manager::DictManager;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    insert_value_into_ap,
};
use cairo_vm::types::relocatable::Relocatable;

use crate::hints::error::HintResult;
use crate::hints::types::HintArgs;
use crate::hints::vars::{Const, Ids, InnerStateToPointerDict, Scope};

pub(crate) fn enter_scope_with_aliases<S: StateReader>(
    HintArgs { exec_scopes, .. }: HintArgs<'_, S>,
) -> HintResult {
    // Note that aliases, execution_helper and os_input do not enter the new scope as they are not
    // needed.
    let dict_manager_str: &str = Scope::DictManager.into();
    let dict_manager: DictManager = exec_scopes.get(dict_manager_str)?;
    let classes_pointer_str: &str = Scope::ClassesPointer.into();
    let classes_pointer: Relocatable = exec_scopes.get(classes_pointer_str)?;
    let state_pointer_str: &str = Scope::StatePointer.into();
    let state_pointer: Relocatable = exec_scopes.get(state_pointer_str)?;
    let aliases_pointer_str: &str = Scope::AliasesPointer.into();
    let aliases_pointer: Relocatable = exec_scopes.get(aliases_pointer_str)?;
    let inner_state_to_pointer_str: &str = Scope::InnerStateToPointer.into();
    let inner_state_to_pointer: InnerStateToPointerDict =
        exec_scopes.get(inner_state_to_pointer_str)?;
    let new_scope = HashMap::from([
        (dict_manager_str.to_string(), any_box!(dict_manager)),
        (classes_pointer_str.to_string(), any_box!(classes_pointer)),
        (state_pointer_str.to_string(), any_box!(state_pointer)),
        (inner_state_to_pointer_str.to_string(), any_box!(inner_state_to_pointer)),
        (aliases_pointer_str.to_string(), any_box!(aliases_pointer)),
    ]);
    exec_scopes.enter_scope(new_scope);
    Ok(())
}

pub(crate) fn guess_alias_ptr<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn update_alias_ptr<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn key_lt_min_alias_alloc_value<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn assert_key_big_enough_for_alias<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn read_alias_from_key<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn write_next_alias_from_key<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn read_alias_counter<S: StateReader>(
    HintArgs { hint_processor, vm, constants, .. }: HintArgs<'_, S>,
) -> HintResult {
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
) -> HintResult {
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
) -> HintResult {
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
) -> HintResult {
    todo!()
}

pub(crate) fn compute_commitments_on_finalized_state_with_aliases<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn guess_inner_state_contract_address_ptr<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn update_inner_state_contract_address_ptr<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn guess_state_ptr<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn update_state_ptr<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn guess_classes_ptr<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn update_classes_ptr<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}
