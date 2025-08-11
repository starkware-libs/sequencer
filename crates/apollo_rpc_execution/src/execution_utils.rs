//! Utilities for executing contracts and transactions.
use std::fs::File;
use std::path::PathBuf;

use apollo_storage::compiled_class::CasmStorageReader;
use apollo_storage::db::{TransactionKind, RO};
use apollo_storage::state::StateStorageReader;
use apollo_storage::{StorageError, StorageResult, StorageTxn};
use blockifier::execution::contract_class::{
    CompiledClassV0,
    CompiledClassV1,
    RunnableCompiledClass,
};
use blockifier::state::cached_state::{CachedState, CommitmentStateDiff, MutRefState};
use blockifier::transaction::objects::TransactionExecutionInfo;
use cairo_vm::types::errors::program_errors::ProgramError;
use indexmap::IndexMap;
use papyrus_common::state::{DeployedContract, ReplacedClass, StorageEntry};
// Expose the tool for creating entry point selectors from function names.
pub use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::state::{StateNumber, StorageKey, ThinStateDiff};
use starknet_types_core::felt::Felt;
use thiserror::Error;

use crate::objects::TransactionTrace;
use crate::state_reader::ExecutionStateReader;
use crate::{ExecutableTransactionInput, ExecutionConfig, ExecutionError, ExecutionResult};

// An error that can occur during the use of the execution utils.
#[derive(Debug, Error)]
pub(crate) enum ExecutionUtilsError {
    #[error(transparent)]
    ProgramError(#[from] ProgramError),
    #[error(transparent)]
    StorageError(#[from] StorageError),
    #[error("Casm table not fully synced")]
    CasmTableNotSynced,
    #[error(transparent)]
    SierraValidationError(starknet_api::StarknetApiError),
}

/// Returns the execution config from the config file.
impl TryFrom<PathBuf> for ExecutionConfig {
    type Error = ExecutionError;

    fn try_from(execution_config_file: PathBuf) -> Result<Self, Self::Error> {
        let file = File::open(execution_config_file).map_err(ExecutionError::ConfigFileError)?;
        serde_json::from_reader(file).map_err(ExecutionError::ConfigSerdeError)
    }
}

pub(crate) fn is_contract_class_declared(
    txn: &StorageTxn<'_, RO>,
    class_hash: &ClassHash,
    state_number: StateNumber,
) -> Result<bool, ExecutionUtilsError> {
    Ok(txn
        .get_state_reader()?
        .get_class_definition_block_number(class_hash)?
        .is_some_and(|block_number| state_number.is_after(block_number)))
}

pub(crate) fn get_contract_class(
    txn: &StorageTxn<'_, RO>,
    class_hash: &ClassHash,
    state_number: StateNumber,
) -> Result<Option<RunnableCompiledClass>, ExecutionUtilsError> {
    match txn.get_state_reader()?.get_class_definition_block_number(class_hash)? {
        Some(block_number) if state_number.is_before(block_number) => return Ok(None),
        Some(_block_number) => {
            let (Some(casm), Some(sierra)) = txn.get_casm_and_sierra(class_hash)? else {
                return Err(ExecutionUtilsError::CasmTableNotSynced);
            };
            let sierra_version =
                sierra.get_sierra_version().map_err(ExecutionUtilsError::SierraValidationError)?;
            return Ok(Some(RunnableCompiledClass::V1(CompiledClassV1::try_from((
                casm,
                sierra_version,
            ))?)));
        }
        None => {}
    };

    let Some(deprecated_class) =
        txn.get_state_reader()?.get_deprecated_class_definition_at(state_number, class_hash)?
    else {
        return Ok(None);
    };
    Ok(Some(RunnableCompiledClass::V0(
        CompiledClassV0::try_from(deprecated_class).map_err(ExecutionUtilsError::ProgramError)?,
    )))
}

/// Given an ExecutableTransactionInput, returns a function that will convert the corresponding
/// TransactionExecutionInfo into the right TransactionTrace variant.
pub fn get_trace_constructor(
    tx: &ExecutableTransactionInput,
) -> fn(TransactionExecutionInfo) -> ExecutionResult<TransactionTrace> {
    match tx {
        ExecutableTransactionInput::Invoke(..) => {
            |execution_info| Ok(TransactionTrace::Invoke(execution_info.try_into()?))
        }
        ExecutableTransactionInput::DeclareV0(..) => {
            |execution_info| Ok(TransactionTrace::Declare(execution_info.try_into()?))
        }
        ExecutableTransactionInput::DeclareV1(..) => {
            |execution_info| Ok(TransactionTrace::Declare(execution_info.try_into()?))
        }
        ExecutableTransactionInput::DeclareV2(..) => {
            |execution_info| Ok(TransactionTrace::Declare(execution_info.try_into()?))
        }
        ExecutableTransactionInput::DeclareV3(..) => {
            |execution_info| Ok(TransactionTrace::Declare(execution_info.try_into()?))
        }
        ExecutableTransactionInput::DeployAccount(..) => {
            |execution_info| Ok(TransactionTrace::DeployAccount(execution_info.try_into()?))
        }
        ExecutableTransactionInput::L1Handler(..) => {
            |execution_info| Ok(TransactionTrace::L1Handler(execution_info.try_into()?))
        }
    }
}

/// Returns the state diff induced by a single transaction. If the transaction
/// is a deprecated Declare, the user is required to pass the class hash of the deprecated class as
/// it is not provided by the blockifier API.
// TODO(Dan, Yair): consider box large elements (because of BadDeclareTransaction) or use ID
// instead.
pub fn induced_state_diff(
    transactional_state: &mut CachedState<MutRefState<'_, CachedState<ExecutionStateReader>>>,
    deprecated_declared_class_hash: Option<ClassHash>,
) -> ExecutionResult<ThinStateDiff> {
    let blockifier_state_diff =
        CommitmentStateDiff::from(transactional_state.to_state_diff()?.state_maps);

    Ok(ThinStateDiff {
        deployed_contracts: blockifier_state_diff.address_to_class_hash,
        storage_diffs: blockifier_state_diff.storage_updates,
        declared_classes: blockifier_state_diff.class_hash_to_compiled_class_hash,
        deprecated_declared_classes: deprecated_declared_class_hash
            .map_or_else(Vec::new, |class_hash| vec![class_hash]),
        nonces: blockifier_state_diff.address_to_nonce,
    })
}

/// Get the storage at the given contract and key in the given state. If there's a given pending
/// storage diffs, apply them on top of the given state.
// TODO(shahak): If the structure of storage diffs changes, remove this function and move its code
// into apollo_rpc.
pub fn get_storage_at<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
    state_number: StateNumber,
    pending_storage_diffs: Option<&IndexMap<ContractAddress, Vec<StorageEntry>>>,
    contract_address: ContractAddress,
    key: StorageKey,
) -> StorageResult<Felt> {
    if let Some(pending_storage_diffs) = pending_storage_diffs {
        if let Some(storage_entries) = pending_storage_diffs.get(&contract_address) {
            if let Some(StorageEntry { key: _, value }) = storage_entries
                .iter()
                .find(|StorageEntry { key: other_key, value: _ }| key == *other_key)
            {
                return Ok(*value);
            }
        }
    }
    txn.get_state_reader()?.get_storage_at(state_number, &contract_address, &key)
}

/// Get the nonce at the given contract in the given state. If there's a given pending nonces
/// update, apply them on top of the given state.
pub fn get_nonce_at<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
    state_number: StateNumber,
    pending_nonces: Option<&IndexMap<ContractAddress, Nonce>>,
    contract_address: ContractAddress,
) -> StorageResult<Option<Nonce>> {
    if let Some(pending_nonces) = pending_nonces {
        if let Some(nonce) = pending_nonces.get(&contract_address) {
            return Ok(Some(*nonce));
        }
    }

    txn.get_state_reader()?.get_nonce_at(state_number, &contract_address)
}

/// Get the class hash of the contract at the given address, if it exists. If there's a given
/// pending deployed contracts, search in them as well.
pub fn get_class_hash_at<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
    state_number: StateNumber,
    pending_deployed_contracts_and_replaced_classes: Option<(
        &Vec<DeployedContract>,
        &Vec<ReplacedClass>,
    )>,
    contract_address: ContractAddress,
) -> StorageResult<Option<ClassHash>> {
    if let Some((pending_deployed_contracts, pending_replaced_classes)) =
        pending_deployed_contracts_and_replaced_classes
    {
        // Searching first in the replaced classes because if the contract was deployed and
        // replaced, the replaced class is the contract's class.
        for ReplacedClass { address, class_hash } in pending_replaced_classes {
            if *address == contract_address {
                return Ok(Some(*class_hash));
            }
        }
        for DeployedContract { address, class_hash } in pending_deployed_contracts {
            if *address == contract_address {
                return Ok(Some(*class_hash));
            }
        }
    }
    txn.get_state_reader()?.get_class_hash_at(state_number, &contract_address)
}
