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
    UseKzgDa,
}

impl From<Scope> for &'static str {
    fn from(scope: Scope) -> &'static str {
        match scope {
            Scope::InitialDict => "initial_dict",
            Scope::DictManager => "dict_manager",
            Scope::DictTracker => "dict_tracker",
            Scope::UseKzgDa => "use_kzg_da",
        }
    }
}

#[derive(Debug)]
pub enum Ids {
    BucketIndex,
    CompressedStart,
    DictPtr,
    FullOutput,
    PrevOffset,
    NextAvailableAlias,
    StateUpdatesStart,
    UseKzgDa,
}

impl From<Ids> for &'static str {
    fn from(ids: Ids) -> &'static str {
        match ids {
            Ids::DictPtr => "dict_ptr",
            Ids::BucketIndex => "bucket_index",
            Ids::CompressedStart => "compressed_start",
            Ids::FullOutput => "full_output",
            Ids::PrevOffset => "prev_offset",
            Ids::NextAvailableAlias => "next_available_alias",
            Ids::StateUpdatesStart => "state_updates_start",
            Ids::UseKzgDa => "use_kzg_da",
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

    pub fn fetch_as<T: TryFrom<Felt>>(
        &self,
        constants: &HashMap<String, Felt>,
    ) -> Result<T, OsHintError>
    where
        <T as TryFrom<Felt>>::Error: std::fmt::Debug,
    {
        let self_felt = self.fetch(constants)?;
        T::try_from(*self_felt).map_err(|error| OsHintError::ConstConversionError {
            variant: *self,
            felt: *self_felt,
            ty: std::any::type_name::<T>().into(),
            reason: format!("{error:?}").into(),
        })
    }

    pub fn get_alias_counter_storage_key(
        constants: &HashMap<String, Felt>,
    ) -> Result<StorageKey, OsHintError> {
        Self::AliasCounterStorageKey.fetch_as(constants)
    }

    pub fn get_alias_contract_address(
        constants: &HashMap<String, Felt>,
    ) -> Result<ContractAddress, OsHintError> {
        Self::AliasContractAddress.fetch_as(constants)
    }
}
