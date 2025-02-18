use blockifier::state::state_api::StateReader;
use blockifier::state::stateful_compression::ALIAS_COUNTER_STORAGE_KEY;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_into_ap;
use cairo_vm::vm::errors::hint_errors::HintError;
use starknet_api::core::ContractAddress;

use crate::hints::error::HintResult;
use crate::hints::types::HintArgs;
use crate::hints::vars::Const;

pub(crate) fn enter_scope_with_aliases(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}

pub(crate) fn get_alias_entry_for_state_update(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}

pub(crate) fn key_lt_min_alias_alloc_value(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}

pub(crate) fn assert_key_big_enough_for_alias(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}

pub(crate) fn read_alias_from_key(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}

pub(crate) fn write_next_alias_from_key(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}

pub(crate) fn read_alias_counter(
    HintArgs { hint_processor, vm, constants, .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    let aliases_contract_address_as_felt = Const::AliasContractAddress.fetch(constants)?;
    let aliases_contract_address = ContractAddress::try_from(aliases_contract_address_as_felt)
        .expect("Failed to convert the alias contract address 0x2 to contract address.");
    let alias_counter = hint_processor
        .execution_helper
        .cached_state
        .get_storage_at(aliases_contract_address, ALIAS_COUNTER_STORAGE_KEY)
        .map_err(|_| HintError::CustomHint("Failed to read from storage.".into()))?;
    insert_value_into_ap(vm, alias_counter)
}

pub(crate) fn initialize_alias_counter(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}

pub(crate) fn update_alias_counter(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}

pub(crate) fn contract_address_le_max_for_compression(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}

pub(crate) fn compute_commitments_on_finalized_state_with_aliases(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}
