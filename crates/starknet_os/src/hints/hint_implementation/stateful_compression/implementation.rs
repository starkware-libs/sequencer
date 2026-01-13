use std::collections::HashMap;

use blockifier::state::state_api::{State, StateReader};
use cairo_vm::any_box;
use starknet_api::core::ContractAddress;
use starknet_types_core::felt::Felt;

use crate::hint_processor::common_hint_processor::CommonHintProcessor;
use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hint_processor::state_update_pointers::{
    get_contract_state_entry_and_storage_ptr,
    StateEntryPtr,
    StoragePtr,
};
use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::hint_implementation::compiled_class::utils::CompiledClassFact;
use crate::hints::types::HintContext;
use crate::hints::vars::{CairoStruct, Const, Ids, Scope};
use crate::vm_utils::{get_address_of_nested_fields, LoadCairoObject};

pub(crate) fn enter_scope_with_aliases(ctx: HintContext<'_>) -> OsHintResult {
    // Note that aliases, execution_helper, state_update_pointers and block_input do not enter the
    // new scope as they are not needed.
    let dict_manager = ctx.exec_scopes.get_dict_manager()?;
    let new_scope = HashMap::from([(Scope::DictManager.into(), any_box!(dict_manager))]);
    ctx.exec_scopes.enter_scope(new_scope);
    Ok(())
}

pub(crate) fn get_class_hash_and_compiled_class_fact<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    // Read n_classes from cairo memory (number of remaining classes to process).
    let n_classes: usize =
        ctx.get_integer(Ids::NClasses)?.try_into().expect("n_classes should fit into usize");

    // Get the class at the appropriate index from block input.
    // Classes are processed from index 0 onwards, and n_classes counts down from total.
    let class_hashes_to_migrate =
        &hint_processor.get_current_execution_helper()?.os_block_input.class_hashes_to_migrate;
    let total_classes = class_hashes_to_migrate.len();
    let index = total_classes - n_classes;

    let (class_hash, casm_hash_v2) = class_hashes_to_migrate
        .get(index)
        .copied()
        .expect("Index should be valid for class_hashes_to_migrate");

    ctx.insert_value(Ids::ClassHash, class_hash.0)?;

    // Use compiled class hash v2 to fetch the casm contract.
    let casm_contract = hint_processor
        .compiled_classes
        .get(&casm_hash_v2)
        .ok_or_else(|| OsHintError::MissingBytecodeSegmentStructure(casm_hash_v2))?;

    // Load the CompiledClassFact into memory and return its pointer.
    let compiled_class_fact =
        CompiledClassFact { compiled_class_hash: &casm_hash_v2, compiled_class: casm_contract };
    let compiled_class_fact_ptr = ctx.vm.add_memory_segment();
    compiled_class_fact.load_into(
        ctx.vm,
        hint_processor.program,
        compiled_class_fact_ptr,
        ctx.constants,
    )?;

    ctx.insert_value(Ids::CompiledClassFact, compiled_class_fact_ptr)?;

    Ok(())
}

pub(crate) fn key_lt_min_alias_alloc_value(mut ctx: HintContext<'_>) -> OsHintResult {
    let key = ctx.get_integer(Ids::Key)?;
    let min_value_for_alias_alloc = *Const::MinValueForAliasAlloc.fetch(ctx.constants)?;
    Ok(ctx
        .insert_value(Ids::KeyLtMinAliasAllocValue, Felt::from(key < min_value_for_alias_alloc))?)
}

pub(crate) fn assert_key_big_enough_for_alias(ctx: HintContext<'_>) -> OsHintResult {
    let key = ctx.get_integer(Ids::Key)?;
    let min_value_for_alias_alloc = *Const::MinValueForAliasAlloc.fetch(ctx.constants)?;
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
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let key = ctx.get_integer(Ids::Key)?;
    let execution_helper = hint_processor.get_current_execution_helper()?;
    let aliases_contract_address = Const::get_alias_contract_address(ctx.constants)?;
    let alias =
        execution_helper.cached_state.get_storage_at(aliases_contract_address, key.try_into()?)?;
    Ok(ctx.insert_value(Ids::PrevValue, alias)?)
}

pub(crate) fn write_next_alias_from_key<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    let key = ctx.get_integer(Ids::Key)?;
    let next_available_alias = ctx.get_integer(Ids::NextAvailableAlias)?;
    let execution_helper = hint_processor.get_mut_current_execution_helper()?;
    let aliases_contract_address = Const::get_alias_contract_address(ctx.constants)?;
    Ok(execution_helper.cached_state.set_storage_at(
        aliases_contract_address,
        key.try_into()?,
        next_available_alias,
    )?)
}

pub(crate) fn read_alias_counter<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let aliases_contract_address = Const::get_alias_contract_address(ctx.constants)?;
    let alias_counter_storage_key = Const::get_alias_counter_storage_key(ctx.constants)?;
    let alias_counter = hint_processor
        .get_current_execution_helper()?
        .cached_state
        .get_storage_at(aliases_contract_address, alias_counter_storage_key)?;
    Ok(ctx.insert_value(Ids::NextAvailableAlias, alias_counter)?)
}

pub(crate) fn initialize_alias_counter<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    let aliases_contract_address = Const::get_alias_contract_address(ctx.constants)?;
    let alias_counter_storage_key = Const::get_alias_counter_storage_key(ctx.constants)?;
    let initial_available_alias = *Const::InitialAvailableAlias.fetch(ctx.constants)?;
    Ok(hint_processor.get_mut_current_execution_helper()?.cached_state.set_storage_at(
        aliases_contract_address,
        alias_counter_storage_key,
        initial_available_alias,
    )?)
}

pub(crate) fn update_alias_counter<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    let aliases_contract_address = Const::get_alias_contract_address(ctx.constants)?;
    let alias_counter_storage_key = Const::get_alias_counter_storage_key(ctx.constants)?;
    let next_available_alias = ctx.get_integer(Ids::NextAvailableAlias)?;
    Ok(hint_processor.get_mut_current_execution_helper()?.cached_state.set_storage_at(
        aliases_contract_address,
        alias_counter_storage_key,
        next_available_alias,
    )?)
}

pub(crate) fn contract_address_le_max_for_compression(mut ctx: HintContext<'_>) -> OsHintResult {
    let contract_address = ctx.get_integer(Ids::ContractAddress)?;
    let max_contract_address = *Const::MaxNonCompressedContractAddress.fetch(ctx.constants)?;
    Ok(ctx.insert_value(
        Ids::ContractAddressLeMaxForCompression,
        Felt::from(contract_address <= max_contract_address),
    )?)
}

pub(crate) fn load_storage_ptr_and_prev_state<'program, CHP: CommonHintProcessor<'program>>(
    hint_processor: &mut CHP,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let key_address = get_address_of_nested_fields(
        ctx.ids_data,
        Ids::StateChanges,
        CairoStruct::DictAccessPtr,
        ctx.vm,
        ctx.ap_tracking,
        &["key"],
        hint_processor.get_program(),
    )?;
    let contract_address =
        ContractAddress(ctx.vm.get_integer(key_address)?.into_owned().try_into()?);
    let (state_entry, storage_ptr) = get_contract_state_entry_and_storage_ptr(
        hint_processor.get_mut_state_update_pointers(),
        ctx.vm,
        contract_address,
    );
    ctx.insert_value(Ids::SquashedPrevState, state_entry.0)?;
    ctx.insert_value(Ids::SquashedStoragePtr, storage_ptr.0)?;

    Ok(())
}

pub(crate) fn update_contract_addr_to_storage_ptr<'program, CHP: CommonHintProcessor<'program>>(
    hint_processor: &mut CHP,
    ctx: HintContext<'_>,
) -> OsHintResult {
    let program = hint_processor.get_program();
    if let Some(state_update_pointers) = hint_processor.get_mut_state_update_pointers() {
        let key_address = get_address_of_nested_fields(
            ctx.ids_data,
            Ids::StateChanges,
            CairoStruct::DictAccessPtr,
            ctx.vm,
            ctx.ap_tracking,
            &["key"],
            program,
        )?;
        let contract_address =
            ContractAddress(ctx.vm.get_integer(key_address)?.into_owned().try_into()?);
        let squashed_new_state = ctx.get_ptr(Ids::SquashedNewState)?;
        let squashed_storage_ptr_end = ctx.get_ptr(Ids::SquashedStoragePtrEnd)?;

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
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let aliases_contract_address = Const::get_alias_contract_address(ctx.constants)?;
    let (state_entry_ptr, storage_ptr) = get_contract_state_entry_and_storage_ptr(
        &mut hint_processor.state_update_pointers,
        ctx.vm,
        aliases_contract_address,
    );
    ctx.insert_value(Ids::PrevAliasesStateEntry, state_entry_ptr.0)?;
    ctx.insert_value(Ids::SquashedAliasesStorageStart, storage_ptr.0)?;
    Ok(())
}

pub(crate) fn update_aliases_contract_to_storage_ptr<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    if let Some(state_update_pointers) = &mut hint_processor.state_update_pointers {
        let aliases_contract_address = Const::get_alias_contract_address(ctx.constants)?;
        let aliases_state_entry_ptr = ctx.get_ptr(Ids::NewAliasesStateEntry)?;
        let aliases_storage_ptr = ctx.get_ptr(Ids::SquashedAliasesStorageEnd)?;
        state_update_pointers.set_contract_state_entry_and_storage_ptr(
            aliases_contract_address,
            StateEntryPtr(aliases_state_entry_ptr),
            StoragePtr(aliases_storage_ptr),
        );
    }
    Ok(())
}
