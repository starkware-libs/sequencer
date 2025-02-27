use std::collections::HashMap;

use cairo_vm::vm::errors::hint_errors::HintError;
use starknet_api::core::ContractAddress;
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::hints::error::OsHintError;

#[derive(Copy, Clone)]
pub(crate) enum Scope {
    CompiledClassFacts,
    DeprecatedClassHashes,
    DictManager,
    DictTracker,
    InitialDict,
    UseKzgDa,
    CommitmentInfoByAddress,
}

impl From<Scope> for &'static str {
    fn from(scope: Scope) -> &'static str {
        match scope {
            Scope::CompiledClassFacts => "compiled_class_facts",
            Scope::DeprecatedClassHashes => "__deprecated_class_hashes",
            Scope::DictManager => "dict_manager",
            Scope::DictTracker => "dict_tracker",
            Scope::CommitmentInfoByAddress => "commitment_info_by_address",
            Scope::InitialDict => "initial_dict",
            Scope::UseKzgDa => "use_kzg_da",
        }
    }
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let scope_string: &'static str = (*self).into();
        write!(f, "{}", scope_string)
    }
}

#[derive(Debug)]
pub enum Ids {
    BucketIndex,
    CompressedStart,
    DictPtr,
    FullOutput,
    NCompiledClassFacts,
    NextAvailableAlias,
    OldBlockHash,
    OldBlockNumber,
    PrevOffset,
    Sha256Ptr,
    StateUpdatesStart,
    UseKzgDa,
    AliasesEntry,
    OsStateUpdate,
}

impl From<Ids> for &'static str {
    fn from(ids: Ids) -> &'static str {
        match ids {
            Ids::BucketIndex => "bucket_index",
            Ids::CompressedStart => "compressed_start",
            Ids::DictPtr => "dict_ptr",
            Ids::FullOutput => "full_output",
            Ids::NCompiledClassFacts => "n_compiled_class_facts",
            Ids::NextAvailableAlias => "next_available_alias",
            Ids::OldBlockHash => "old_block_hash",
            Ids::OldBlockNumber => "old_block_number",
            Ids::OsStateUpdate => "os_state_update",
            Ids::PrevOffset => "prev_offset",
            Ids::Sha256Ptr => "sha256_ptr",
            Ids::StateUpdatesStart => "state_updates_start",
            Ids::UseKzgDa => "use_kzg_da",
            Ids::AliasesEntry => "aliases_entry",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Const {
    AliasContractAddress,
    AliasCounterStorageKey,
    InitialAvailableAlias,
    StoredBlockHashBuffer,
}

impl From<Const> for &'static str {
    fn from(constant: Const) -> &'static str {
        match constant {
            Const::AliasContractAddress => "ALIAS_CONTRACT_ADDRESS",
            Const::AliasCounterStorageKey => "ALIAS_COUNTER_STORAGE_KEY",
            Const::InitialAvailableAlias => "INITIAL_AVAILABLE_ALIAS",
            Const::StoredBlockHashBuffer => "STORED_BLOCK_HASH_BUFFER",
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
            reason: format!("{error:?}"),
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
pub enum CairoStruct {
    DictAccess,
    OsStateUpdate,
}

impl From<CairoStruct> for &'static str {
    fn from(struct_name: CairoStruct) -> Self {
        match struct_name {
            CairoStruct::DictAccess => "starkware.cairo.common.dict_access.DictAccess",
            CairoStruct::OsStateUpdate => "starkware.starknet.core.os.state.state.OsStateUpdate",
        }
    }
}
