use std::collections::HashMap;

use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_into_ap;
use cairo_vm::vm::errors::hint_errors::HintError;
use starknet_api::core::ContractAddress;
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::hints::error::HintResult;
use crate::hints::types::HintArgs;
use crate::hints::vars::Const;

pub(crate) fn enter_scope_with_aliases<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn get_alias_entry_for_state_update<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
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
    let aliases_contract_address = get_alias_contract_address(constants)?;
    let alias_counter_storage_key = get_alias_counter_storage_key(constants)?;
    let alias_counter = hint_processor
        .execution_helper
        .cached_state
        .get_storage_at(aliases_contract_address, alias_counter_storage_key)
        .map_err(|_| {
            HintError::CustomHint(
                format!(
                    "Failed to read alias contract {aliases_contract_address} counter at key \
                     {alias_counter_storage_key:?}."
                )
                .into(),
            )
        })?;
    insert_value_into_ap(vm, alias_counter)
}

pub(crate) fn initialize_alias_counter<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn update_alias_counter<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
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

fn get_alias_contract_address(
    constants: &HashMap<String, Felt>,
) -> Result<ContractAddress, HintError> {
    let alias_contract_address_as_felt = *Const::AliasContractAddress.fetch(constants)?;
    Ok(ContractAddress::try_from(alias_contract_address_as_felt).unwrap_or_else(|_| {
        panic!(
            "Failed to convert the alias contract address {alias_contract_address_as_felt:?} to \
             contract address."
        )
    }))
}

fn get_alias_counter_storage_key(
    constants: &HashMap<String, Felt>,
) -> Result<StorageKey, HintError> {
    let alias_counter_storage_key = *Const::AliasContractAddress.fetch(constants)?;
    Ok(StorageKey::try_from(alias_counter_storage_key).unwrap_or_else(|_| {
        panic!(
            "Failed to convert the alias counter storage key {alias_counter_storage_key:?} to \
             storage key."
        )
    }))
}
