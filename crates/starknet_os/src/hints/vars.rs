use std::collections::HashMap;

use cairo_vm::vm::errors::hint_errors::HintError;
use starknet_api::core::ContractAddress;
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::hints::error::OsHintError;

#[derive(Copy, Clone)]
pub(crate) enum Scope {
    BytecodeSegmentStructure,
    Case,
    CommitmentInfoByAddress,
    CompiledClass,
    CompiledClassFacts,
    CompiledClassHash,
    ComponentHashes,
    DeprecatedClassHashes,
    DictManager,
    DictTracker,
    InitialDict,
    IsDeprecated,
    Preimage,
    SerializeDataAvailabilityCreatePages,
    Transactions,
    UseKzgDa,
}

impl From<Scope> for &'static str {
    fn from(scope: Scope) -> &'static str {
        match scope {
            Scope::BytecodeSegmentStructure => "bytecode_segment_structure",
            Scope::Case => "case",
            Scope::CommitmentInfoByAddress => "commitment_info_by_address",
            Scope::CompiledClass => "compiled_class",
            Scope::CompiledClassFacts => "compiled_class_facts",
            Scope::CompiledClassHash => "compiled_class_hash",
            Scope::ComponentHashes => "component_hashes",
            Scope::DeprecatedClassHashes => "__deprecated_class_hashes",
            Scope::DictManager => "dict_manager",
            Scope::DictTracker => "dict_tracker",
            Scope::InitialDict => "initial_dict",
            Scope::IsDeprecated => "is_deprecated",
            Scope::Preimage => "preimage",
            Scope::SerializeDataAvailabilityCreatePages => {
                "__serialize_data_availability_create_pages__"
            }
            Scope::Transactions => "transactions",
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

impl From<Scope> for String {
    fn from(scope: Scope) -> String {
        let scope_as_str: &str = scope.into();
        scope_as_str.to_string()
    }
}

#[derive(Debug)]
pub enum Ids {
    AliasesEntry,
    Bit,
    BucketIndex,
    BuiltinCosts,
    CompiledClass,
    CompiledClassFact,
    CompressedDst,
    CompressedStart,
    ContractAddress,
    ContractStateChanges,
    DataEnd,
    DataStart,
    DecompressedDst,
    DictPtr,
    Edge,
    ElmBound,
    ExecutionContext,
    FinalRoot,
    FullOutput,
    Hash,
    Height,
    InitialCarriedOutputs,
    InitialRoot,
    NCompiledClassFacts,
    NTxs,
    NewLength,
    NextAvailableAlias,
    NewStateEntry,
    Node,
    OldBlockHash,
    OldBlockNumber,
    OsStateUpdate,
    PackedFelt,
    PrevOffset,
    PrevValue,
    RangeCheck96Ptr,
    Request,
    Sha256Ptr,
    StateEntry,
    StateUpdatesStart,
    SyscallPtr,
    UseKzgDa,
    Value,
}

impl From<Ids> for &'static str {
    fn from(ids: Ids) -> &'static str {
        match ids {
            Ids::AliasesEntry => "aliases_entry",
            Ids::Bit => "bit",
            Ids::BucketIndex => "bucket_index",
            Ids::BuiltinCosts => "builtin_costs",
            Ids::CompiledClass => "compiled_class",
            Ids::CompiledClassFact => "compiled_class_fact",
            Ids::CompressedDst => "compressed_dst",
            Ids::CompressedStart => "compressed_start",
            Ids::ContractAddress => "contract_address",
            Ids::ContractStateChanges => "contract_state_changes",
            Ids::DataEnd => "data_end",
            Ids::DataStart => "data_start",
            Ids::DecompressedDst => "decompressed_dst",
            Ids::DictPtr => "dict_ptr",
            Ids::Edge => "edge",
            Ids::ElmBound => "elm_bound",
            Ids::ExecutionContext => "execution_context",
            Ids::FinalRoot => "final_root",
            Ids::FullOutput => "full_output",
            Ids::Hash => "hash",
            Ids::Height => "height",
            Ids::InitialCarriedOutputs => "initial_carried_outputs",
            Ids::InitialRoot => "initial_root",
            Ids::NCompiledClassFacts => "n_compiled_class_facts",
            Ids::NTxs => "n_txs",
            Ids::NewLength => "new_length",
            Ids::NextAvailableAlias => "next_available_alias",
            Ids::NewStateEntry => "new_state_entry",
            Ids::Node => "node",
            Ids::OldBlockHash => "old_block_hash",
            Ids::OldBlockNumber => "old_block_number",
            Ids::OsStateUpdate => "os_state_update",
            Ids::PackedFelt => "packed_felt",
            Ids::PrevOffset => "prev_offset",
            Ids::PrevValue => "prev_value",
            Ids::RangeCheck96Ptr => "range_check96_ptr",
            Ids::Request => "request",
            Ids::Sha256Ptr => "sha256_ptr",
            Ids::StateEntry => "state_entry",
            Ids::StateUpdatesStart => "state_updates_start",
            Ids::SyscallPtr => "syscall_ptr",
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
    CompiledClassVersion,
    InitialAvailableAlias,
    MerkleHeight,
    StoredBlockHashBuffer,
}

impl From<Const> for &'static str {
    fn from(constant: Const) -> &'static str {
        match constant {
            Const::AliasContractAddress => "ALIAS_CONTRACT_ADDRESS",
            Const::AliasCounterStorageKey => "ALIAS_COUNTER_STORAGE_KEY",
            Const::BlockHashContractAddress => "BLOCK_HASH_CONTRACT_ADDRESS",
            Const::CompiledClassVersion => "COMPILED_CLASS_VERSION",
            Const::InitialAvailableAlias => "INITIAL_AVAILABLE_ALIAS",
            Const::MerkleHeight => "MERKLE_HEIGHT",
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
    CompiledClass,
    CompiledClassEntryPoint,
    CompiledClassFact,
    DeprecatedCompiledClass,
    DeprecatedCompiledClassFact,
    DictAccess,
    ExecutionContext,
    NodeEdge,
    OsStateUpdate,
    StorageReadPtr,
    StorageReadRequestPtr,
}

impl From<CairoStruct> for &'static str {
    fn from(struct_name: CairoStruct) -> Self {
        match struct_name {
            CairoStruct::CompiledClass => {
                "starkware.starknet.core.os.contract_class.compiled_class.CompiledClass"
            }
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
            CairoStruct::ExecutionContext => {
                "starkware.starknet.core.os.execution.execute_entry_point.ExecutionContext"
            }
            CairoStruct::NodeEdge => "starkware.cairo.common.patricia_utils.NodeEdge",
            CairoStruct::OsStateUpdate => "starkware.starknet.core.os.state.state.OsStateUpdate",
            CairoStruct::StorageReadPtr => "starkware.starknet.common.syscalls.StorageRead*",
            CairoStruct::StorageReadRequestPtr => {
                "starkware.starknet.core.os.storage.StorageReadRequest*"
            }
        }
    }
}
