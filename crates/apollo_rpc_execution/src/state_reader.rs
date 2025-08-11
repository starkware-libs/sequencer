#[cfg(test)]
#[path = "state_reader_test.rs"]
mod state_reader_test;

use std::cell::Cell;

use apollo_class_manager_types::SharedClassManagerClient;
use apollo_storage::state::StateStorageReader;
use apollo_storage::{StorageError, StorageReader};
use blockifier::execution::contract_class::{
    CompiledClassV0,
    CompiledClassV1,
    RunnableCompiledClass,
};
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{StateReader as BlockifierStateReader, StateResult};
use papyrus_common::pending_classes::{ApiContractClass, PendingClassesTrait};
use papyrus_common::state::DeclaredClassHashEntry;
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::{StateNumber, StorageKey};
use starknet_types_core::felt::Felt;
use tokio::runtime::Handle;

use crate::execution_utils::{
    self,
    get_contract_class,
    is_contract_class_declared,
    ExecutionUtilsError,
};
use crate::objects::PendingData;

/// A view into the state at a specific state number.
pub struct ExecutionStateReader {
    pub storage_reader: StorageReader,
    pub state_number: StateNumber,
    pub maybe_pending_data: Option<PendingData>,
    // We want to return a custom error when missing a compiled class, but we need to return
    // Blockifier's error, so we store the missing class's hash in case of error.
    pub missing_compiled_class: Cell<Option<ClassHash>>,
    pub class_manager_handle: Option<(SharedClassManagerClient, Handle)>,
}

impl BlockifierStateReader for ExecutionStateReader {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt> {
        execution_utils::get_storage_at(
            &self.storage_reader.begin_ro_txn().map_err(storage_err_to_state_err)?,
            self.state_number,
            self.maybe_pending_data.as_ref().map(|pending_data| &pending_data.storage_diffs),
            contract_address,
            key,
        )
        .map_err(storage_err_to_state_err)
    }

    // Returns the default value if the contract address is not found.
    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        Ok(execution_utils::get_nonce_at(
            &self.storage_reader.begin_ro_txn().map_err(storage_err_to_state_err)?,
            self.state_number,
            self.maybe_pending_data.as_ref().map(|pending_data| &pending_data.nonces),
            contract_address,
        )
        .map_err(storage_err_to_state_err)?
        .unwrap_or_default())
    }

    // Returns the default value if the contract address is not found.
    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        Ok(execution_utils::get_class_hash_at(
            &self.storage_reader.begin_ro_txn().map_err(storage_err_to_state_err)?,
            self.state_number,
            self.maybe_pending_data.as_ref().map(|pending_data| {
                (&pending_data.deployed_contracts, &pending_data.replaced_classes)
            }),
            contract_address,
        )
        .map_err(storage_err_to_state_err)?
        .unwrap_or_default())
    }

    /// Note: when self.class_manager_handle is [Some] this function must be run in a
    /// tokio::spawn_blocking() thread, because of the blocking code.
    fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        if let Some(pending_classes) =
            self.maybe_pending_data.as_ref().map(|pending_data| &pending_data.classes)
        {
            if let Some(api_contract_class) = pending_classes.get_class(class_hash) {
                match api_contract_class {
                    ApiContractClass::ContractClass(sierra) => {
                        if let Some(pending_casm) = pending_classes.get_compiled_class(class_hash) {
                            let sierra_version = sierra.get_sierra_version()?;
                            let runnable_compiled_class = RunnableCompiledClass::V1(
                                CompiledClassV1::try_from((pending_casm, sierra_version))
                                    .map_err(StateError::ProgramError)?,
                            );
                            return Ok(runnable_compiled_class);
                        }
                    }
                    ApiContractClass::DeprecatedContractClass(pending_deprecated_class) => {
                        return Ok(RunnableCompiledClass::V0(
                            CompiledClassV0::try_from(pending_deprecated_class)
                                .map_err(StateError::ProgramError)?,
                        ));
                    }
                }
            }
        }

        if let Some((class_manager_client, run_time_handle)) = &self.class_manager_handle {
            let contract_class = run_time_handle
                .block_on(class_manager_client.get_executable(class_hash))
                .map_err(|e| StateError::StateReadError(e.to_string()))?
                .ok_or(StateError::UndeclaredClassHash(class_hash))?;

            return match contract_class {
                ContractClass::V1(casm_contract_class) => {
                    let is_declared = is_contract_class_declared(
                        &self.storage_reader.begin_ro_txn().map_err(storage_err_to_state_err)?,
                        &class_hash,
                        self.state_number,
                    )
                    .map_err(|e| StateError::StateReadError(e.to_string()))?;

                    if is_declared {
                        Ok(RunnableCompiledClass::V1(casm_contract_class.try_into()?))
                    } else {
                        Err(StateError::UndeclaredClassHash(class_hash))
                    }
                }
                // TODO(shahak): Verify cairo0 as well after get_class_definition_block_number is
                // fixed.
                ContractClass::V0(deprecated_contract_class) => {
                    Ok(RunnableCompiledClass::V0(deprecated_contract_class.try_into()?))
                }
            };
        }

        match get_contract_class(
            &self.storage_reader.begin_ro_txn().map_err(storage_err_to_state_err)?,
            &class_hash,
            self.state_number,
        ) {
            Ok(Some(contract_class)) => Ok(contract_class),
            Ok(None) => Err(StateError::UndeclaredClassHash(class_hash)),
            Err(ExecutionUtilsError::CasmTableNotSynced) => {
                self.missing_compiled_class.set(Some(class_hash));
                Err(StateError::StateReadError("Casm table not fully synced".to_string()))
            }
            Err(ExecutionUtilsError::ProgramError(err)) => Err(StateError::ProgramError(err)),
            Err(ExecutionUtilsError::StorageError(err)) => Err(storage_err_to_state_err(err)),
            Err(ExecutionUtilsError::SierraValidationError(err)) => {
                Err(StateError::StarknetApiError(err))
            }
        }
    }

    fn get_compiled_class_hash(&self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        if let Some(pending_data) = &self.maybe_pending_data {
            for DeclaredClassHashEntry { class_hash: other_class_hash, compiled_class_hash } in
                &pending_data.declared_classes
            {
                if class_hash == *other_class_hash {
                    return Ok(*compiled_class_hash);
                }
            }
        }
        let block_number = self
            .storage_reader
            .begin_ro_txn()
            .map_err(storage_err_to_state_err)?
            .get_state_reader()
            .map_err(storage_err_to_state_err)?
            .get_class_definition_block_number(&class_hash)
            .map_err(storage_err_to_state_err)?
            .ok_or(StateError::UndeclaredClassHash(class_hash))?;

        let state_diff = self
            .storage_reader
            .begin_ro_txn()
            .map_err(storage_err_to_state_err)?
            .get_state_diff(block_number)
            .map_err(storage_err_to_state_err)?
            .ok_or(StateError::StateReadError(format!(
                "Inner storage error. Missing state diff at block {block_number}."
            )))?;

        let compiled_class_hash = state_diff.declared_classes.get(&class_hash).ok_or(
            StateError::StateReadError(format!(
                "Inner storage error. Missing class declaration at block {block_number}, class \
                 {class_hash}."
            )),
        )?;

        Ok(*compiled_class_hash)
    }
}

// Converts a storage error to the error type of the state reader.
fn storage_err_to_state_err(err: StorageError) -> StateError {
    StateError::StateReadError(err.to_string())
}
