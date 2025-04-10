use std::collections::HashMap;

use cairo_vm::vm::errors::hint_errors::HintError;
use starknet_api::core::ContractAddress;
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::hints::error::OsHintError;

/// Defines an enum with a conversion to a `&'static str`. If no explicit string is provided for a
/// variant, the variant is converted to snake case.
///
/// Example:
/// ```
/// # #[macro_use] extern crate starknet_os;
/// define_string_enum! {
///     #[derive(Copy, Clone)]
///     pub enum X {
///         (HelloWorld),
///         (GoodbyeWorld, "GB"),
///     }
/// }
///
/// let hello_world: &str = X::HelloWorld.into();
/// let goodbye_world: &str = X::GoodbyeWorld.into();
/// assert_eq!("hello_world", hello_world);
/// assert_eq!("GB", goodbye_world);
/// ```
#[macro_export]
macro_rules! define_string_enum {
    (
        $(#[$cfgs:meta])*
        $visibility:vis enum $enum_name:ident {
            $(($variant:ident $(, $variant_str:expr)?)),+ $(,)?
        }
    ) => {
        $(#[$cfgs])*
        $visibility enum $enum_name {
            $($variant),+
        }

        impl From<$enum_name> for &'static str {
            fn from(value: $enum_name) -> Self {
                match value {
                    $($enum_name::$variant => string_or_snake_case!($variant $(, $variant_str)?),)+
                }
            }
        }
    };
}

/// Expands to the snake case representation of the given ident, or simply the explicit string (if
/// provided).
///
/// Example:
///
/// Input:
/// ```
/// # #[macro_use] extern crate starknet_os;
/// assert_eq!("hello_world", string_or_snake_case!(HelloWorld));
/// assert_eq!("GB", string_or_snake_case!(GoodbyeWorld, "GB"));
/// ```
#[macro_export]
macro_rules! string_or_snake_case {
    // Explicit string provided.
    ($variant:ident, $variant_str:expr) => {
        $variant_str
    };
    // No explicit string provided: snake case.
    ($variant:ident) => {
        paste::paste! { stringify!( [< $variant:snake >] ) }
    };
}

define_string_enum! {
    #[derive(Copy, Clone)]
    pub(crate) enum Scope {
        (BytecodeSegments),
        (BytecodeSegmentStructure),
        (BytecodeSegmentStructures),
        (Case),
        (CommitmentInfoByAddress),
        (CompiledClass),
        (CompiledClassFacts),
        (CompiledClassHash),
        (ComponentHashes),
        (DeprecatedClassHashes, "__deprecated_class_hashes"),
        (DictManager),
        (DictTracker),
        (InitialDict),
        (IsDeprecated),
        (Preimage),
        (SerializeDataAvailabilityCreatePages, "__serialize_data_availability_create_pages__"),
        (SyscallHandlerType),
        (Transactions),
        (Tx),
        (UseKzgDa),
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
        (AliasesEntry),
        (Bit),
        (BucketIndex),
        (BuiltinCosts),
        (BuiltinParams),
        (BuiltinPtrs),
        (CompiledClass),
        (CompiledClassFact),
        (CompiledClassFacts),
        (CompressedDst),
        (CompressedStart),
        (ContractAddress),
        (ContractStateChanges),
        (DaSize),
        (DataEnd),
        (DataStart),
        (DecompressedDst),
        (DictPtr),
        (Edge),
        (ElmBound),
        (Evals),
        (ExecutionContext),
        (FinalRoot),
        (FinalSquashedContractStateChangesEnd),
        (FinalSquashedContractStateChangesStart),
        (FullOutput),
        (Hash),
        (Height),
        (InitialCarriedOutputs),
        (InitialRoot),
        (IsLeaf),
        (KzgCommitments),
        (Low),
        (MaxGas),
        (NBlobs),
        (NBuiltins),
        (NCompiledClassFacts),
        (NSelectedBuiltins),
        (NTxs),
        (NewAliasesStateEntry),
        (NewLength),
        (NextAvailableAlias),
        (NewStateEntry),
        (Node),
        (OldBlockHash),
        (OldBlockNumber),
        (OsStateUpdate),
        (PackedFelt),
        (PrevOffset),
        (PrevAliasesStateEntry),
        (PrevValue),
        (RangeCheck96Ptr, "range_check96_ptr"),
        (RangeCheckPtr),
        (RemainingGas),
        (ResourceBounds),
        (ReturnBuiltinPtrs),
        (Request),
        (Res),
        (SelectedEncodings),
        (SelectedPtrs),
        (Sha256Ptr, "sha256_ptr"),
        (SquashedAliasesStorageEnd),
        (SquashedAliasesStorageStart),
        (SquashedDict),
        (SquashedDictEnd),
        (StateEntry),
        (StateUpdatesStart),
        (SyscallPtr),
        (TransactionHash),
        (TxType),
        (UseKzgDa),
        (Value),
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
        (
            EntryPointInitialBudget,
            "starkware.starknet.core.os.constants.ENTRY_POINT_INITIAL_BUDGET"
        ),
        (InitialAvailableAlias, "starkware.starknet.core.os.state.aliases.INITIAL_AVAILABLE_ALIAS"),
        (
            MaxNonCompressedContractAddress,
            "starkware.starknet.core.os.state.aliases.MAX_NON_COMPRESSED_CONTRACT_ADDRESS"
        ),
        (MerkleHeight, "starkware.starknet.core.os.state.commitment.MERKLE_HEIGHT"),
        (StoredBlockHashBuffer, "starkware.starknet.core.os.constants.STORED_BLOCK_HASH_BUFFER"),
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
        (BuiltinParamsPtr, "starkware.starknet.core.os.builtins.BuiltinParams*"),
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
            DeprecatedCompiledClassFactPtr,
            "starkware.starknet.core.os.contract_class.deprecated_compiled_class.\
            DeprecatedCompiledClassFact*"
        ),
        (
            DeprecatedCompiledClassPtr,
            "starkware.starknet.core.os.contract_class.deprecated_compiled_class.\
            DeprecatedCompiledClass*"
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
        (StateEntry, "starkware.starknet.core.os.state.commitment.StateEntry"),
        (StorageReadPtr, "starkware.starknet.common.syscalls.StorageRead*"),
        (StorageReadRequestPtr, "starkware.starknet.core.os.storage.StorageReadRequest*"),
        (StorageWritePtr, "starkware.starknet.common.syscalls.StorageWriteRequest*"),
    }
}
