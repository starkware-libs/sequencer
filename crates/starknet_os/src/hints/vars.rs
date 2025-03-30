use std::collections::HashMap;

use cairo_vm::vm::errors::hint_errors::HintError;
use starknet_api::core::ContractAddress;
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::hints::error::OsHintError;

/// Defines an enum with a conversion to a `&'static str`.
///
/// Example:
///
/// Input:
/// ```
/// # #[macro_use] extern crate starknet_os; fn main() {
/// define_string_enum! {
///     #[derive(Copy, Clone)]
///     pub enum X {
///         (Y, "y"),
///         (Z, "z"),
///     }
/// }
/// # }
/// ```
///
/// Output:
/// ```
/// #[derive(Copy, Clone)]
/// pub enum X {
///     Y,
///     Z,
/// }
///
/// impl From<X> for &'static str {
///     fn from(value: X) -> Self {
///         match value {
///             X::Y => "y",
///             X::Z => "z",
///         }
///     }
/// }
/// ```
#[macro_export]
macro_rules! define_string_enum {
    (
        $(#[$cfgs:meta])*
        $visibility:vis enum $enum_name:ident {
            $(($variant:ident, $variant_str:expr)),+ $(,)?
        }
    ) => {
        $(#[$cfgs])*
        $visibility enum $enum_name {
            $($variant),+
        }

        impl From<$enum_name> for &'static str {
            fn from(value: $enum_name) -> Self {
                match value {
                    $($enum_name::$variant => $variant_str,)+
                }
            }
        }
    };
}

define_string_enum! {
    #[derive(Copy, Clone)]
    pub(crate) enum Scope {
        (BytecodeSegments, "bytecode_segments"),
        (BytecodeSegmentStructure, "bytecode_segment_structure"),
        (BytecodeSegmentStructures, "bytecode_segment_structures"),
        (Case, "case"),
        (CommitmentInfoByAddress, "commitment_info_by_address"),
        (CompiledClass, "compiled_class"),
        (CompiledClassFacts, "compiled_class_facts"),
        (CompiledClassHash, "compiled_class_hash"),
        (ComponentHashes, "component_hashes"),
        (DeprecatedClassHashes, "__deprecated_class_hashes"),
        (DictManager, "dict_manager"),
        (DictTracker, "dict_tracker"),
        (InitialDict, "initial_dict"),
        (IsDeprecated, "is_deprecated"),
        (Preimage, "preimage"),
        (SerializeDataAvailabilityCreatePages, "__serialize_data_availability_create_pages__"),
        (StateUpdatePointers, "state_update_pointers"),
        (SyscallHandlerType, "syscall_handler_type"),
        (Transactions, "transactions"),
        (Tx, "tx"),
        (UseKzgDa, "use_kzg_da"),
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

define_string_enum! {
    #[derive(Debug, Clone)]
    pub enum Ids {
        (AliasesEntry, "aliases_entry"),
        (Bit, "bit"),
        (BucketIndex, "bucket_index"),
        (BuiltinCosts, "builtin_costs"),
        (BuiltinPtrs, "builtin_ptrs"),
        (CompiledClass, "compiled_class"),
        (CompiledClassFact, "compiled_class_fact"),
        (CompressedDst, "compressed_dst"),
        (CompressedStart, "compressed_start"),
        (ContractAddress, "contract_address"),
        (ContractStateChanges, "contract_state_changes"),
        (DaSize, "da_size"),
        (DataEnd, "data_end"),
        (DataStart, "data_start"),
        (DecompressedDst, "decompressed_dst"),
        (DictPtr, "dict_ptr"),
        (Edge, "edge"),
        (ElmBound, "elm_bound"),
        (Evals, "evals"),
        (ExecutionContext, "execution_context"),
        (FinalRoot, "final_root"),
        (FullOutput, "full_output"),
        (Hash, "hash"),
        (Height, "height"),
        (InitialCarriedOutputs, "initial_carried_outputs"),
        (InitialRoot, "initial_root"),
        (IsLeaf, "is_leaf"),
        (KzgCommitments, "kzg_commitments"),
        (Low, "low"),
        (MaxGas, "max_gas"),
        (NBlobs, "n_blobs"),
        (NCompiledClassFacts, "n_compiled_class_facts"),
        (NTxs, "n_txs"),
        (NewLength, "new_length"),
        (NextAvailableAlias, "next_available_alias"),
        (NewStateEntry, "new_state_entry"),
        (Node, "node"),
        (OldBlockHash, "old_block_hash"),
        (OldBlockNumber, "old_block_number"),
        (OsStateUpdate, "os_state_update"),
        (PackedFelt, "packed_felt"),
        (PrevOffset, "prev_offset"),
        (PrevValue, "prev_value"),
        (RangeCheck96Ptr, "range_check96_ptr"),
        (RangeCheckPtr, "range_check_ptr"),
        (RemainingGas, "remaining_gas"),
        (ResourceBounds, "resource_bounds,"),
        (Request, "request"),
        (Res, "res"),
        (Sha256Ptr, "sha256_ptr"),
        (StateEntry, "state_entry"),
        (StateUpdatesStart, "state_updates_start"),
        (SyscallPtr, "syscall_ptr"),
        (TransactionHash, "transaction_hash"),
        (TxType, "tx_type"),
        (UseKzgDa, "use_kzg_da"),
        (Value, "value"),
    }
}

define_string_enum! {
    #[derive(Clone, Copy, Debug)]
    pub enum Const {
        (AliasContractAddress, "starkware.starknet.core.os.constants.ALIAS_CONTRACT_ADDRESS"),
        (
            AliasCounterStorageKey,
            "starkware.starknet.core.os.state.aliases.ALIAS_COUNTER_STORAGE_KEY"
        ),
        (Base, "starkware.starknet.core.os.data_availability.bls_field.BASE"),
        (BlobLength, "starkware.starknet.core.os.data_availability.commitment.BLOB_LENGTH"),
        (
            BlockHashContractAddress,
            "starkware.starknet.core.os.constants.BLOCK_HASH_CONTRACT_ADDRESS"
        ),
        (
            CompiledClassVersion,
            "starkware.starknet.core.os.contract_class.compiled_class.COMPILED_CLASS_VERSION"
        ),
        (
            DeprecatedCompiledClassVersion,
            "starkware.starknet.core.os.contract_class.deprecated_compiled_class.\
            DEPRECATED_COMPILED_CLASS_VERSION"
        ),
        (InitialAvailableAlias, "starkware.starknet.core.os.state.aliases.INITIAL_AVAILABLE_ALIAS"),
        (MerkleHeight, "starkware.starknet.core.os.state.commitment.MERKLE_HEIGHT"),
        (StoredBlockHashBuffer, "starkware.starknet.core.os.constants.STORED_BLOCK_HASH_BUFFER"),
        (
            EntryPointInitialBudget,
            "starkware.starknet.core.os.constants.ENTRY_POINT_INITIAL_BUDGET"
        ),
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

define_string_enum! {
    #[derive(Copy, Clone)]
    pub enum CairoStruct {
        (BigInt3, "starkware.starknet.core.os.data_availability.bls_field.BigInt3"),
        (BuiltinPointersPtr, "starkware.starknet.core.os.builtins.BuiltinPointers*"),
        (CompiledClass, "starkware.starknet.core.os.contract_class.compiled_class.CompiledClass"),
        (
            CompiledClassEntryPoint,
            "starkware.starknet.core.os.contract_class.compiled_class.CompiledClassEntryPoint"
        ),
        (
            CompiledClassFact,
            "starkware.starknet.core.os.contract_class.compiled_class.CompiledClassFact"
        ),
        (
            DeprecatedCompiledClass,
            "starkware.starknet.core.os.contract_class.deprecated_compiled_class.\
            DeprecatedCompiledClass"
        ),
        (
            DeprecatedCompiledClassFact,
            "starkware.starknet.core.os.contract_class.deprecated_compiled_class.\
            DeprecatedCompiledClassFact"
        ),
        (
            DeprecatedContractEntryPoint,
            "starkware.starknet.core.os.contract_class.deprecated_compiled_class.\
            DeprecatedContractEntryPoint"
        ),
        (DictAccess, "starkware.cairo.common.dict_access.DictAccess"),
        (
            ExecutionContext,
            "starkware.starknet.core.os.execution.execute_entry_point.ExecutionContext"
        ),
        (NodeEdge, "starkware.cairo.common.patricia_utils.NodeEdge"),
        (NonSelectableBuiltins, "starkware.starknet.core.os.builtins.NonSelectableBuiltins"),
        (OsStateUpdate, "starkware.starknet.core.os.state.state.OsStateUpdate"),
        (ResourceBounds, "starkware.starknet.common.new_syscalls.ResourceBounds"),
        (SelectableBuiltins, "starkware.starknet.core.os.builtins.SelectableBuiltins"),
        (StateEntry, "starkware.starknet.core.os.state.state.StateEntry"),
        (StorageReadPtr, "starkware.starknet.common.syscalls.StorageRead*"),
        (StorageReadRequestPtr, "starkware.starknet.core.os.storage.StorageReadRequest*"),
        (StorageWritePtr, "starkware.starknet.common.syscalls.StorageWriteRequest*"),
    }
}
