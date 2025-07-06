#![allow(non_local_definitions)]

use std::str::FromStr;

use apollo_state_reader::papyrus_state::PapyrusReader;
use blockifier::blockifier::config::{ContractClassManagerConfig, TransactionExecutorConfig};
use blockifier::blockifier::transaction_executor::{
    BlockExecutionSummary,
    TransactionExecutor,
    TransactionExecutorError,
};
use blockifier::blockifier_versioned_constants::VersionedConstants;
use blockifier::bouncer::BouncerConfig;
use blockifier::context::{BlockContext, ChainInfo, FeeTokenAddresses};
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::state_reader_and_contract_manager::StateReaderAndContractManager;
use blockifier::transaction::objects::TransactionExecutionInfo;
use blockifier::transaction::transaction_execution::Transaction;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList};
use pyo3::{FromPyObject, PyAny, Python};
use shared_execution_objects::central_objects::CentralTransactionExecutionInfo;
use starknet_api::block::BlockNumber;
use starknet_api::contract_class::SierraVersion;
use starknet_api::core::{ChainId, ContractAddress};
use starknet_types_core::felt::Felt;

use crate::errors::{NativeBlockifierError, NativeBlockifierResult};
use crate::py_objects::{
    PyBouncerConfig,
    PyCasmHashComputationData,
    PyConcurrencyConfig,
    PyContractClassManagerConfig,
    PyVersionedConstantsOverrides,
};
use crate::py_state_diff::{PyBlockInfo, PyStateDiff};
use crate::py_transaction::{py_tx, PyClassInfo, PY_TX_PARSING_ERR};
use crate::py_utils::{int_to_chain_id, into_block_number_hash_pair, PyFelt};
use crate::storage::{
    PapyrusStorage,
    RawDeclaredClassMapping,
    RawDeprecatedDeclaredClassMapping,
    Storage,
    StorageConfig,
};

pub(crate) type RawTransactionExecutionResult = Vec<u8>;
const RESULT_SERIALIZE_ERR: &str = "Failed serializing execution info.";

/// Return type for the finalize method containing state diffs, bouncer weights, and CASM hash
/// computation data.
type FinalizeResult = (
    PyStateDiff,
    Option<PyStateDiff>,
    Py<PyBytes>,
    PyCasmHashComputationData,
    PyCasmHashComputationData,
);

#[cfg(test)]
#[path = "py_block_executor_test.rs"]
mod py_block_executor_test;

fn serialize_tx_execution_info(
    tx_execution_info: TransactionExecutionInfo,
) -> RawTransactionExecutionResult {
    let central_tx_execution_info = CentralTransactionExecutionInfo::from(tx_execution_info);
    serde_json::to_vec(&central_tx_execution_info).expect(RESULT_SERIALIZE_ERR)
}

#[pyclass]
pub struct PyBlockExecutor {
    pub bouncer_config: BouncerConfig,
    pub tx_executor_config: TransactionExecutorConfig,
    pub chain_info: ChainInfo,
    pub versioned_constants: VersionedConstants,
    pub tx_executor: Option<TransactionExecutor<StateReaderAndContractManager<PapyrusReader>>>,
    /// `Send` trait is required for `pyclass` compatibility as Python objects must be threadsafe.
    pub storage: Box<dyn Storage + Send>,
    pub contract_class_manager: ContractClassManager,
}

#[pymethods]
impl PyBlockExecutor {
    #[new]
    #[pyo3(signature = (bouncer_config, concurrency_config, contract_class_manager_config, os_config, target_storage_config, py_versioned_constants_overrides, stack_size))]
    pub fn create(
        bouncer_config: PyBouncerConfig,
        concurrency_config: PyConcurrencyConfig,
        contract_class_manager_config: PyContractClassManagerConfig,
        os_config: PyOsConfig,
        target_storage_config: StorageConfig,
        py_versioned_constants_overrides: PyVersionedConstantsOverrides,
        stack_size: usize,
    ) -> Self {
        log::debug!("Initializing Block Executor...");
        let storage =
            PapyrusStorage::new(target_storage_config).expect("Failed to initialize storage.");
        let versioned_constants =
            VersionedConstants::get_versioned_constants(py_versioned_constants_overrides.into());
        log::debug!("Initialized Block Executor.");

        Self {
            bouncer_config: bouncer_config.try_into().expect("Failed to parse bouncer config."),
            tx_executor_config: TransactionExecutorConfig {
                concurrency_config: concurrency_config.into(),
                stack_size,
            },
            chain_info: os_config.into_chain_info(),
            versioned_constants,
            tx_executor: None,
            storage: Box::new(storage),
            contract_class_manager: ContractClassManager::start(
                contract_class_manager_config.into(),
            ),
        }
    }

    // Transaction Execution API.

    /// Initializes the transaction executor for the given block.
    #[pyo3(signature = (next_block_info, old_block_number_and_hash))]
    fn setup_block_execution(
        &mut self,
        next_block_info: PyBlockInfo,
        old_block_number_and_hash: Option<(u64, PyFelt)>,
    ) -> NativeBlockifierResult<()> {
        // Create block context.
        let block_context = BlockContext::new(
            next_block_info.try_into()?,
            self.chain_info.clone(),
            self.versioned_constants.clone(),
            self.bouncer_config.clone(),
        );
        let next_block_number = block_context.block_info().block_number;

        // Create state reader.
        let state_reader = self.get_aligned_reader(next_block_number);
        // Create and set executor.
        self.tx_executor = Some(TransactionExecutor::pre_process_and_create(
            state_reader,
            block_context,
            into_block_number_hash_pair(old_block_number_and_hash),
            self.tx_executor_config.clone(),
        )?);
        Ok(())
    }

    fn teardown_block_execution(&mut self) {
        self.tx_executor = None;
    }

    #[pyo3(signature = (tx, optional_py_class_info))]
    pub fn execute(
        &mut self,
        tx: &PyAny,
        optional_py_class_info: Option<PyClassInfo>,
    ) -> NativeBlockifierResult<Py<PyBytes>> {
        let tx: Transaction = py_tx(tx, optional_py_class_info).expect(PY_TX_PARSING_ERR);
        let (tx_execution_info, _state_diff) = self.tx_executor().execute(&tx)?;

        // Serialize and convert to PyBytes.
        let serialized_tx_execution_info = serialize_tx_execution_info(tx_execution_info);
        Ok(Python::with_gil(|py| PyBytes::new(py, &serialized_tx_execution_info).into()))
    }

    /// Executes the given transactions on the Blockifier state.
    /// Stops if and when there is no more room in the block, and returns the executed transactions'
    /// results as a PyList of (success (bool), serialized result (bytes)) tuples.
    #[pyo3(signature = (txs_with_class_infos))]
    pub fn execute_txs(
        &mut self,
        txs_with_class_infos: Vec<(&PyAny, Option<PyClassInfo>)>,
    ) -> Py<PyList> {
        // Parse Py transactions.
        let txs: Vec<Transaction> = txs_with_class_infos
            .into_iter()
            .map(|(tx, optional_py_class_info)| {
                py_tx(tx, optional_py_class_info).expect(PY_TX_PARSING_ERR)
            })
            .collect();

        // Run.
        let results =
            Python::with_gil(|py| py.allow_threads(|| self.tx_executor().execute_txs(&txs, None)));

        // Process results.
        // TODO(Yoni, 15/5/2024): serialize concurrently.
        let serialized_results: Vec<(bool, RawTransactionExecutionResult)> = results
            .into_iter()
            // Note: there might be less results than txs (if there is no room for all of them).
            .map(|result| match result {
                Ok((tx_execution_info, _state_diff)) => (
                    true,
                    serialize_tx_execution_info(
                        tx_execution_info,
                    ),
                ),
                Err(error) => (false, serialize_failure_reason(error)),
            })
            .collect();

        // Convert to Py types and allocate it on Python's heap, to be visible for Python's
        // garbage collector.
        Python::with_gil(|py| {
            let py_serialized_results: Vec<(bool, Py<PyBytes>)> = serialized_results
                .into_iter()
                .map(|(success, execution_result)| {
                    // Note that PyList converts the inner elements recursively, yet the default
                    // conversion of the execution result (Vec<u8>) is to a list of integers, which
                    // might be less efficient than bytes.
                    (success, PyBytes::new(py, &execution_result).into())
                })
                .collect();
            PyList::new(py, py_serialized_results).into()
        })
    }

    /// Returns the state diff, the stateful-compressed state diff and the block weights.
    pub fn finalize(&mut self) -> NativeBlockifierResult<FinalizeResult> {
        log::debug!("Finalizing execution...");
        let BlockExecutionSummary {
            state_diff,
            compressed_state_diff,
            bouncer_weights,
            casm_hash_computation_data_sierra_gas,
            casm_hash_computation_data_proving_gas,
        } = self.tx_executor().finalize()?;
        let py_state_diff = PyStateDiff::from(state_diff);
        let py_compressed_state_diff = compressed_state_diff.map(PyStateDiff::from);
        let py_casm_hash_computation_data_sierra_gas = casm_hash_computation_data_sierra_gas.into();
        let py_casm_hash_computation_data_proving_gas =
            casm_hash_computation_data_proving_gas.into();

        let serialized_block_weights =
            serde_json::to_vec(&bouncer_weights).expect("Failed serializing bouncer weights.");
        let raw_block_weights =
            Python::with_gil(|py| PyBytes::new(py, &serialized_block_weights).into());

        log::debug!("Finalized execution.");

        Ok((
            py_state_diff,
            py_compressed_state_diff,
            raw_block_weights,
            py_casm_hash_computation_data_sierra_gas,
            py_casm_hash_computation_data_proving_gas,
        ))
    }

    // Storage Alignment API.

    /// Appends state diff and block header into Papyrus storage.
    // Previous block ID can either be a block hash (starting from a Papyrus snapshot), or a
    // sequential ID (throughout sequencing).
    #[pyo3(signature = (
        block_id,
        previous_block_id,
        py_block_info,
        py_state_diff,
        declared_class_hash_to_class,
        deprecated_declared_class_hash_to_class
    ))]
    pub fn append_block(
        &mut self,
        block_id: u64,
        previous_block_id: Option<PyFelt>,
        py_block_info: PyBlockInfo,
        py_state_diff: PyStateDiff,
        declared_class_hash_to_class: RawDeclaredClassMapping,
        deprecated_declared_class_hash_to_class: RawDeprecatedDeclaredClassMapping,
    ) -> NativeBlockifierResult<()> {
        self.storage.append_block(
            block_id,
            previous_block_id,
            py_block_info,
            py_state_diff,
            declared_class_hash_to_class,
            deprecated_declared_class_hash_to_class,
        )
    }

    /// Returns the next block number, for which block header was not yet appended.
    /// Block header stream is usually ahead of the state diff stream, so this is the indicative
    /// marker.
    pub fn get_header_marker(&self) -> NativeBlockifierResult<u64> {
        self.storage.get_header_marker()
    }

    /// Returns the unique identifier of the given block number in bytes.
    #[pyo3(signature = (block_number))]
    fn get_block_id_at_target(&self, block_number: u64) -> NativeBlockifierResult<Option<PyFelt>> {
        let optional_block_id_bytes = self.storage.get_block_id(block_number)?;
        let Some(block_id_bytes) = optional_block_id_bytes else { return Ok(None) };

        let mut block_id_fixed_bytes = [0_u8; 32];
        block_id_fixed_bytes.copy_from_slice(&block_id_bytes);

        Ok(Some(PyFelt(Felt::from_bytes_be(&block_id_fixed_bytes))))
    }

    #[pyo3(signature = (source_block_number))]
    pub fn validate_aligned(&self, source_block_number: u64) {
        self.storage.validate_aligned(source_block_number);
    }

    /// Atomically reverts block header and state diff of given block number.
    /// If header exists without a state diff (usually the case), only the header is reverted.
    /// (this is true for every partial existence of information at tables).
    #[pyo3(signature = (block_number))]
    pub fn revert_block(&mut self, block_number: u64) -> NativeBlockifierResult<()> {
        // Clear global class cache, to properly revert classes declared in the reverted block.
        self.contract_class_manager.clear();
        self.storage.revert_block(block_number)
    }

    /// Deallocate the transaction executor and close storage connections.
    pub fn close(&mut self) {
        log::debug!("Closing Block Executor.");
        // If the block was not finalized (due to some exception occuring _in Python_), we need
        // to deallocate the transaction executor here to prevent leaks.
        self.teardown_block_execution();
        self.storage.close();
    }

    #[pyo3(signature = (concurrency_config, contract_class_manager_config, os_config, path, max_state_diff_size, stack_size, min_sierra_version))]
    #[staticmethod]
    fn create_for_testing(
        concurrency_config: PyConcurrencyConfig,
        contract_class_manager_config: PyContractClassManagerConfig,
        os_config: PyOsConfig,
        path: std::path::PathBuf,
        max_state_diff_size: usize,
        stack_size: usize,
        min_sierra_version: Option<String>,
    ) -> Self {
        use blockifier::bouncer::BouncerWeights;
        // TODO(Meshi, 01/01/2025): Remove this once we fix all python tests that re-declare cairo0
        // contracts.
        let mut versioned_constants = VersionedConstants::latest_constants().clone();
        versioned_constants.disable_cairo0_redeclaration = false;

        if let Some(min_sierra_version) = min_sierra_version {
            versioned_constants.min_sierra_version_for_sierra_gas =
                SierraVersion::from_str(&min_sierra_version)
                    .expect("failed to parse sierra version.");
        }

        Self {
            bouncer_config: BouncerConfig {
                block_max_capacity: BouncerWeights {
                    state_diff_size: max_state_diff_size,
                    ..BouncerWeights::max()
                },
                ..BouncerConfig::max()
            },
            tx_executor_config: TransactionExecutorConfig {
                concurrency_config: concurrency_config.into(),
                stack_size,
            },
            storage: Box::new(PapyrusStorage::new_for_testing(path, &os_config.chain_id)),
            chain_info: os_config.into_chain_info(),
            versioned_constants,
            tx_executor: None,
            contract_class_manager: ContractClassManager::start(
                contract_class_manager_config.into(),
            ),
        }
    }
}

impl PyBlockExecutor {
    pub fn tx_executor(
        &mut self,
    ) -> &mut TransactionExecutor<StateReaderAndContractManager<PapyrusReader>> {
        self.tx_executor.as_mut().expect("Transaction executor should be initialized")
    }

    fn get_aligned_reader(
        &self,
        next_block_number: BlockNumber,
    ) -> StateReaderAndContractManager<PapyrusReader> {
        // Full-node storage must be aligned to the Python storage before initializing a reader.
        self.storage.validate_aligned(next_block_number.0);
        let papyrus_reader = PapyrusReader::new(self.storage.reader().clone(), next_block_number);

        StateReaderAndContractManager {
            state_reader: papyrus_reader,
            contract_class_manager: self.contract_class_manager.clone(),
        }
    }

    pub fn create_for_testing_with_storage(storage: impl Storage + Send + 'static) -> Self {
        Self {
            bouncer_config: BouncerConfig::max(),
            tx_executor_config: TransactionExecutorConfig::create_for_testing(true),
            storage: Box::new(storage),
            chain_info: ChainInfo::default(),
            versioned_constants: VersionedConstants::latest_constants().clone(),
            tx_executor: None,
            contract_class_manager: ContractClassManager::start(
                ContractClassManagerConfig::default(),
            ),
        }
    }

    #[cfg(test)]
    pub(crate) fn native_create_for_testing(
        concurrency_config: PyConcurrencyConfig,
        contract_class_manager_config: PyContractClassManagerConfig,
        os_config: PyOsConfig,
        path: std::path::PathBuf,
        max_state_diff_size: usize,
        stack_size: usize,
    ) -> Self {
        Self::create_for_testing(
            concurrency_config,
            contract_class_manager_config,
            os_config,
            path,
            max_state_diff_size,
            stack_size,
            None,
        )
    }
}

#[derive(Clone, FromPyObject)]
pub struct PyOsConfig {
    #[pyo3(from_py_with = "int_to_chain_id")]
    pub chain_id: ChainId,
    pub deprecated_fee_token_address: PyFelt,
    pub fee_token_address: PyFelt,
}

impl PyOsConfig {
    pub fn into_chain_info(self) -> ChainInfo {
        ChainInfo::try_from(self).expect("Failed to convert chain info.")
    }
}

impl TryFrom<PyOsConfig> for ChainInfo {
    type Error = NativeBlockifierError;

    fn try_from(py_os_config: PyOsConfig) -> Result<Self, Self::Error> {
        Ok(Self {
            chain_id: py_os_config.chain_id,
            fee_token_addresses: FeeTokenAddresses {
                eth_fee_token_address: ContractAddress::try_from(
                    py_os_config.deprecated_fee_token_address.0,
                )?,
                strk_fee_token_address: ContractAddress::try_from(
                    py_os_config.fee_token_address.0,
                )?,
            },
        })
    }
}

impl Default for PyOsConfig {
    fn default() -> Self {
        Self {
            chain_id: ChainId::Other("".to_string()),
            deprecated_fee_token_address: Default::default(),
            fee_token_address: Default::default(),
        }
    }
}

fn serialize_failure_reason(error: TransactionExecutorError) -> RawTransactionExecutionResult {
    // TODO(Yoni, 1/7/2024): re-consider this serialization.
    serde_json::to_vec(&format!("{error}")).expect(RESULT_SERIALIZE_ERR)
}
