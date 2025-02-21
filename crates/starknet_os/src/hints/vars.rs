use std::collections::HashMap;

use cairo_vm::vm::errors::hint_errors::HintError;
use starknet_api::core::ContractAddress;
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::hints::error::OsHintError;

pub(crate) enum Scope {
    InitialDict,
    DictManager,
    DictTracker,
}

impl From<Scope> for &'static str {
    fn from(scope: Scope) -> &'static str {
        match scope {
            Scope::InitialDict => "initial_dict",
            Scope::DictManager => "dict_manager",
            Scope::DictTracker => "dict_tracker",
        }
    }
}

pub(crate) enum Ids {
    BucketIndex,
    DictPtr,
    PrevOffset,
    NextAvailableAlias,
}

impl From<Ids> for &'static str {
    fn from(ids: Ids) -> &'static str {
        match ids {
            Ids::DictPtr => "dict_ptr",
            Ids::BucketIndex => "bucket_index",
            Ids::PrevOffset => "prev_offset",
            Ids::NextAvailableAlias => "next_available_alias",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Const {
    AliasContractAddress,
    InitialAvailableAlias,
    AliasCounterStorageKey,
}

impl From<Const> for &'static str {
    fn from(constant: Const) -> &'static str {
        match constant {
            Const::AliasContractAddress => "ALIAS_CONTRACT_ADDRESS",
            Const::InitialAvailableAlias => "INITIAL_AVAILABLE_ALIAS",
            Const::AliasCounterStorageKey => "ALIAS_COUNTER_STORAGE_KEY",
        }
    }
}

impl Const {
    pub fn fetch<'a>(&self, constants: &'a HashMap<String, Felt>) -> Result<&'a Felt, HintError> {
        let identifier = (*self).into();
        constants.get(identifier).ok_or(HintError::MissingConstant(Box::new(identifier)))
    }

    pub fn get_alias_counter_storage_key(
        constants: &HashMap<String, Felt>,
    ) -> Result<StorageKey, OsHintError> {
        let alias_counter_storage_key = *Self::AliasCounterStorageKey.fetch(constants)?;
        Ok(StorageKey::try_from(alias_counter_storage_key).map_err(|_| {
            OsHintError::ConstConversionError {
                variant: Self::AliasCounterStorageKey,
                felt: alias_counter_storage_key,
                ty: std::any::type_name::<StorageKey>().into(),
            }
        })?)
    }

    pub fn get_alias_contract_address(
        constants: &HashMap<String, Felt>,
    ) -> Result<ContractAddress, HintError> {
        let alias_contract_address_as_felt = *Self::AliasContractAddress.fetch(constants)?;
        Ok(ContractAddress::try_from(alias_contract_address_as_felt).map_err(|_| {
            OsHintError::ConstConversionError {
                variant: Self::AliasContractAddress,
                felt: alias_contract_address_as_felt,
                ty: std::any::type_name::<ContractAddress>().into(),
            }
        })?)
    }
}
