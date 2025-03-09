use std::collections::HashMap;

use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::errors::hint_errors::HintError;
use starknet_api::core::ContractAddress;
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::hints::error::OsHintError;

#[derive(Copy, Clone)]
pub(crate) enum Scope {
    CompiledClass,
    CommitmentInfoByAddress,
    CompiledClassFacts,
    CompiledClassHash,
    DeprecatedClassHashes,
    DictManager,
    DictTracker,
    InitialDict,
    InnerStateToPointer,
    StateUpdateTreePointers,
    UseKzgDa,
}

impl From<Scope> for &'static str {
    fn from(scope: Scope) -> &'static str {
        match scope {
            Scope::CommitmentInfoByAddress => "commitment_info_by_address",
            Scope::CompiledClass => "compiled_class",
            Scope::CompiledClassFacts => "compiled_class_facts",
            Scope::CompiledClassHash => "compiled_class_hash",
            Scope::DeprecatedClassHashes => "__deprecated_class_hashes",
            Scope::DictManager => "dict_manager",
            Scope::DictTracker => "dict_tracker",
            Scope::InitialDict => "initial_dict",
            Scope::InnerStateToPointer => "inner_state_to_pointer",
            Scope::StateUpdateTreePointers => "state_update_tree_pointers",
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
    AliasesEntry,
    BucketIndex,
    CompiledClass,
    CompiledClassFact,
    CompressedStart,
    ContractAddress,
    ContractStateChanges,
    DictPtr,
    FullOutput,
    Hash,
    NCompiledClassFacts,
    NTxs,
    NextAvailableAlias,
    OldBlockHash,
    OldBlockNumber,
    OsStateUpdate,
    PrevOffset,
    RangeCheck96Ptr,
    Request,
    Sha256Ptr,
    StateEntry,
    StateUpdatesStart,
    UseKzgDa,
    Value,
}

impl From<Ids> for &'static str {
    fn from(ids: Ids) -> &'static str {
        match ids {
            Ids::AliasesEntry => "aliases_entry",
            Ids::BucketIndex => "bucket_index",
            Ids::CompiledClass => "compiled_class",
            Ids::CompiledClassFact => "compiled_class_fact",
            Ids::CompressedStart => "compressed_start",
            Ids::ContractAddress => "contract_address",
            Ids::ContractStateChanges => "contract_state_changes",
            Ids::DictPtr => "dict_ptr",
            Ids::FullOutput => "full_output",
            Ids::Hash => "hash",
            Ids::NCompiledClassFacts => "n_compiled_class_facts",
            Ids::NTxs => "n_txs",
            Ids::NextAvailableAlias => "next_available_alias",
            Ids::OldBlockHash => "old_block_hash",
            Ids::OldBlockNumber => "old_block_number",
            Ids::OsStateUpdate => "os_state_update",
            Ids::PrevOffset => "prev_offset",
            Ids::RangeCheck96Ptr => "range_check96_ptr",
            Ids::Request => "request",
            Ids::Sha256Ptr => "sha256_ptr",
            Ids::StateEntry => "state_entry",
            Ids::StateUpdatesStart => "state_updates_start",
            Ids::UseKzgDa => "use_kzg_da",
            Ids::Value => "value",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Const {
    AliasContractAddress,
    AliasCounterStorageKey,
    BlockHashContractAddress,
    InitialAvailableAlias,
    StoredBlockHashBuffer,
}

impl From<Const> for &'static str {
    fn from(constant: Const) -> &'static str {
        match constant {
            Const::AliasContractAddress => "ALIAS_CONTRACT_ADDRESS",
            Const::AliasCounterStorageKey => "ALIAS_COUNTER_STORAGE_KEY",
            Const::BlockHashContractAddress => "BLOCK_HASH_CONTRACT_ADDRESS",
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
        T::try_from(*self_felt).map_err(|error| OsHintError::ConstConversion {
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

#[derive(Copy, Clone)]
pub enum CairoStruct {
    CompiledClassEntryPoint,
    CompiledClassFact,
    DeprecatedCompiledClass,
    DeprecatedCompiledClassFact,
    DictAccess,
    OsStateUpdate,
    StorageReadRequestPtr,
}

impl From<CairoStruct> for &'static str {
    fn from(struct_name: CairoStruct) -> Self {
        match struct_name {
            CairoStruct::CompiledClassEntryPoint => {
                "starkware.starknet.core.os.contract_class.compiled_class.CompiledClassEntryPoint"
            }
            CairoStruct::CompiledClassFact => {
                "starkware.starknet.core.os.contract_class.compiled_class.CompiledClassFact"
            }
            CairoStruct::DeprecatedCompiledClass => {
                "starkware.starknet.core.os.contract_class.deprecated_compiled_class.\
                 DeprecatedCompiledClass"
            }
            CairoStruct::DeprecatedCompiledClassFact => {
                "starkware.starknet.core.os.contract_class.deprecated_compiled_class.\
                 DeprecatedCompiledClassFact"
            }
            CairoStruct::DictAccess => "starkware.cairo.common.dict_access.DictAccess",
            CairoStruct::OsStateUpdate => "starkware.starknet.core.os.state.state.OsStateUpdate",
            CairoStruct::StorageReadRequestPtr => {
                "starkware.starknet.core.os.storage.StorageReadRequest*"
            }
        }
    }
}
// TODO(Meshi): Move to a more appropriate place.
pub(crate) type InnerStateToPointerDict = HashMap<ContractAddress, Relocatable>;
