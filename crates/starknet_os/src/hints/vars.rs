use std::collections::HashMap;

use cairo_vm::vm::errors::hint_errors::HintError;
use starknet_api::core::ContractAddress;
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::hints::error::OsHintError;

#[derive(Copy, Clone)]
pub(crate) enum Scope {
    BytecodeSegments,
    BytecodeSegmentStructure,
    BytecodeSegmentStructures,
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
    StateUpdatePointers,
    SyscallHandlerType,
    Transactions,
    Tx,
    UseKzgDa,
}

impl From<Scope> for &'static str {
    fn from(scope: Scope) -> &'static str {
        match scope {
            Scope::BytecodeSegments => "bytecode_segments",
            Scope::BytecodeSegmentStructure => "bytecode_segment_structure",
            Scope::BytecodeSegmentStructures => "bytecode_segment_structures",
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
            Scope::StateUpdatePointers => "state_update_pointers",
            Scope::SyscallHandlerType => "syscall_handler_type",
            Scope::Transactions => "transactions",
            Scope::Tx => "tx",
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

#[derive(Debug, Clone)]
pub enum Ids {
    AliasesEntry,
    Bit,
    BucketIndex,
    BuiltinCosts,
    BuiltinParams,
    BuiltinPtrs,
    CompiledClass,
    CompiledClassFact,
    CompressedDst,
    CompressedStart,
    ContractAddress,
    ContractStateChanges,
    DaSize,
    DataEnd,
    DataStart,
    DecompressedDst,
    DictPtr,
    Edge,
    ElmBound,
    EntryPointReturnValues,
    Evals,
    ExecutionContext,
    FinalRoot,
    FullOutput,
    Hash,
    Height,
    InitialCarriedOutputs,
    InitialRoot,
    IsLeaf,
    KzgCommitments,
    Low,
    MaxGas,
    NBlobs,
    NBuiltins,
    NCompiledClassFacts,
    NSelectedBuiltins,
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
    RangeCheckPtr,
    RemainingGas,
    ResourceBounds,
    ReturnBuiltinPtrs,
    Request,
    Res,
    SelectedEncodings,
    SelectedPtrs,
    Sha256Ptr,
    StateEntry,
    StateUpdatesStart,
    SyscallPtr,
    TransactionHash,
    TxType,
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
            Ids::BuiltinParams => "builtin_params",
            Ids::BuiltinPtrs => "builtin_ptrs",
            Ids::CompiledClass => "compiled_class",
            Ids::CompiledClassFact => "compiled_class_fact",
            Ids::CompressedDst => "compressed_dst",
            Ids::CompressedStart => "compressed_start",
            Ids::ContractAddress => "contract_address",
            Ids::ContractStateChanges => "contract_state_changes",
            Ids::DaSize => "da_size",
            Ids::DataEnd => "data_end",
            Ids::DataStart => "data_start",
            Ids::DecompressedDst => "decompressed_dst",
            Ids::DictPtr => "dict_ptr",
            Ids::Edge => "edge",
            Ids::ElmBound => "elm_bound",
            Ids::EntryPointReturnValues => "entry_point_return_values",
            Ids::Evals => "evals",
            Ids::ExecutionContext => "execution_context",
            Ids::FinalRoot => "final_root",
            Ids::FullOutput => "full_output",
            Ids::Hash => "hash",
            Ids::Height => "height",
            Ids::InitialCarriedOutputs => "initial_carried_outputs",
            Ids::InitialRoot => "initial_root",
            Ids::IsLeaf => "is_leaf",
            Ids::KzgCommitments => "kzg_commitments",
            Ids::Low => "low",
            Ids::MaxGas => "max_gas",
            Ids::NBlobs => "n_blobs",
            Ids::NBuiltins => "n_builtins",
            Ids::NCompiledClassFacts => "n_compiled_class_facts",
            Ids::NSelectedBuiltins => "n_selected_builtins",
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
            Ids::RangeCheckPtr => "range_check_ptr",
            Ids::RemainingGas => "remaining_gas",
            Ids::ResourceBounds => "resource_bounds,",
            Ids::ReturnBuiltinPtrs => "return_builtin_ptrs",
            Ids::Request => "request",
            Ids::Res => "res",
            Ids::SelectedEncodings => "selected_encodings",
            Ids::SelectedPtrs => "selected_ptrs",
            Ids::Sha256Ptr => "sha256_ptr",
            Ids::StateEntry => "state_entry",
            Ids::StateUpdatesStart => "state_updates_start",
            Ids::SyscallPtr => "syscall_ptr",
            Ids::TransactionHash => "transaction_hash",
            Ids::TxType => "tx_type",
            Ids::UseKzgDa => "use_kzg_da",
            Ids::Value => "value",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Const {
    AliasContractAddress,
    AliasCounterStorageKey,
    Base,
    BlobLength,
    BlockHashContractAddress,
    CompiledClassVersion,
    DeprecatedCompiledClassVersion,
    EntryPointInitialBudget,
    InitialAvailableAlias,
    MaxNonCompressedContractAddress,
    MerkleHeight,
    StoredBlockHashBuffer,
}

impl From<Const> for &'static str {
    fn from(constant: Const) -> &'static str {
        match constant {
            Const::AliasContractAddress => {
                "starkware.starknet.core.os.constants.ALIAS_CONTRACT_ADDRESS"
            }
            Const::AliasCounterStorageKey => {
                "starkware.starknet.core.os.state.aliases.ALIAS_COUNTER_STORAGE_KEY"
            }
            Const::Base => "starkware.starknet.core.os.data_availability.bls_field.BASE",
            Const::BlobLength => {
                "starkware.starknet.core.os.data_availability.commitment.BLOB_LENGTH"
            }
            Const::BlockHashContractAddress => {
                "starkware.starknet.core.os.constants.BLOCK_HASH_CONTRACT_ADDRESS"
            }
            Const::CompiledClassVersion => {
                "starkware.starknet.core.os.contract_class.compiled_class.COMPILED_CLASS_VERSION"
            }
            Const::DeprecatedCompiledClassVersion => {
                "starkware.starknet.core.os.contract_class.deprecated_compiled_class.\
                 DEPRECATED_COMPILED_CLASS_VERSION"
            }
            Const::InitialAvailableAlias => {
                "starkware.starknet.core.os.state.aliases.INITIAL_AVAILABLE_ALIAS"
            }
            Const::MaxNonCompressedContractAddress => {
                "starkware.starknet.core.os.state.aliases.MAX_NON_COMPRESSED_CONTRACT_ADDRESS"
            }
            Const::MerkleHeight => "starkware.starknet.core.os.state.commitment.MERKLE_HEIGHT",
            Const::StoredBlockHashBuffer => {
                "starkware.starknet.core.os.constants.STORED_BLOCK_HASH_BUFFER"
            }
            Const::EntryPointInitialBudget => {
                "starkware.starknet.core.os.constants.ENTRY_POINT_INITIAL_BUDGET"
            }
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
    BigInt3,
    BuiltinParamsPtr,
    BuiltinPointersPtr,
    CompiledClass,
    CompiledClassEntryPoint,
    CompiledClassFact,
    DeprecatedCompiledClass,
    DeprecatedCompiledClassFact,
    DeprecatedContractEntryPoint,
    DictAccess,
    EntryPointReturnValues,
    ExecutionContext,
    NodeEdge,
    NonSelectableBuiltins,
    OsStateUpdate,
    ResourceBounds,
    SelectableBuiltins,
    StateEntry,
    StorageReadPtr,
    StorageReadRequestPtr,
    StorageWritePtr,
}

impl From<CairoStruct> for &'static str {
    fn from(struct_name: CairoStruct) -> Self {
        match struct_name {
            CairoStruct::BigInt3 => {
                "starkware.starknet.core.os.data_availability.bls_field.BigInt3"
            }
            CairoStruct::BuiltinParamsPtr => "starkware.starknet.core.os.builtins.BuiltinParams*",
            CairoStruct::BuiltinPointersPtr => {
                "starkware.starknet.core.os.builtins.BuiltinPointers*"
            }
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
            CairoStruct::DeprecatedContractEntryPoint => {
                "starkware.starknet.core.os.contract_class.deprecated_compiled_class.\
                 DeprecatedContractEntryPoint"
            }
            CairoStruct::DictAccess => "starkware.cairo.common.dict_access.DictAccess",
            CairoStruct::EntryPointReturnValues => {
                "starkware.starknet.core.os.execution.execute_entry_point.EntryPointReturnValues*"
            }
            CairoStruct::ExecutionContext => {
                "starkware.starknet.core.os.execution.execute_entry_point.ExecutionContext"
            }
            CairoStruct::NodeEdge => "starkware.cairo.common.patricia_utils.NodeEdge",
            CairoStruct::NonSelectableBuiltins => {
                "starkware.starknet.core.os.builtins.NonSelectableBuiltins"
            }
            CairoStruct::OsStateUpdate => "starkware.starknet.core.os.state.state.OsStateUpdate",
            CairoStruct::ResourceBounds => "starkware.starknet.common.new_syscalls.ResourceBounds",
            CairoStruct::SelectableBuiltins => {
                "starkware.starknet.core.os.builtins.SelectableBuiltins"
            }
            CairoStruct::StateEntry => "starkware.starknet.core.os.state.state.StateEntry",
            CairoStruct::StorageReadPtr => "starkware.starknet.common.syscalls.StorageRead*",
            CairoStruct::StorageReadRequestPtr => {
                "starkware.starknet.core.os.storage.StorageReadRequest*"
            }
            CairoStruct::StorageWritePtr => {
                "starkware.starknet.common.syscalls.StorageWriteRequest*"
            }
        }
    }
}
