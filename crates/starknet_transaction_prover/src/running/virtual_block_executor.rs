use std::collections::HashSet;

use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::blockifier::transaction_executor::{
    TransactionExecutionOutput,
    TransactionExecutor,
};
use blockifier::blockifier_versioned_constants::VersionedConstants;
use blockifier::bouncer::BouncerConfig;
use blockifier::context::{BlockContext, ChainInfo};
use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::state::cached_state::{CachedState, StateMaps};
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::global_cache::CompiledClasses;
use blockifier::state::state_api::{StateReader, StateResult};
use blockifier::state::state_reader_and_contract_manager::{
    FetchCompiledClasses,
    StateReaderAndContractManager,
};
use blockifier::transaction::account_transaction::ExecutionFlags;
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use blockifier_reexecution::state_reader::rpc_objects::{BlockHeader, BlockId};
use blockifier_reexecution::state_reader::rpc_state_reader::RpcStateReader;
use serde::{Deserialize, Serialize};
use serde_json::json;
use starknet_api::block::{BlockHash, BlockInfo};
use starknet_api::block_hash::block_hash_calculator::{concat_counts, BlockHeaderCommitments};
use starknet_api::contract_class::SierraVersion;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::rpc_transaction::{RpcInvokeTransaction, RpcInvokeTransactionV3, RpcTransaction};
use starknet_api::state::StorageKey;
use starknet_api::transaction::fields::Fee;
use starknet_api::transaction::{InvokeTransaction, MessageToL1, Transaction, TransactionHash};
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;
use starknet_api::StarknetApiError;
use starknet_types_core::felt::Felt;
use tracing::{error, warn};

use crate::errors::VirtualBlockExecutorError;
use crate::running::serde_utils::deserialize_rpc_initial_reads;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct RpcVirtualBlockExecutorConfig {
    /// When enabled, prefetches state by simulating transactions before execution, reducing RPC
    /// calls during proving.
    #[serde(default)]
    pub(crate) prefetch_state: bool,
    /// Bouncer configuration for virtual block capacity limits.
    /// Client-side limits may differ from Starknet limits.
    #[serde(default)]
    // TODO(Aviv): Decide on the default value.
    pub(crate) bouncer_config: BouncerConfig,
    /// When true, use the latest versioned constants instead of the ones matching the block's
    /// Starknet version. The OS currently always runs with the latest constants.
    // TODO(Aviv): Reconsider using latest versioned constants if the OS becomes backward
    // compatible with older versioned constants.
    #[serde(default = "default_use_latest_versioned_constants")]
    pub(crate) use_latest_versioned_constants: bool,
}

fn default_use_latest_versioned_constants() -> bool {
    true
}

impl Default for RpcVirtualBlockExecutorConfig {
    fn default() -> Self {
        Self {
            prefetch_state: true,
            bouncer_config: BouncerConfig::default(),
            use_latest_versioned_constants: true,
        }
    }
}

/// Captures execution data for a virtual block (multiple transactions).
///
/// This struct contains all the execution data needed for proof generation.
pub(crate) struct BaseBlockInfo {
    pub(crate) block_context: BlockContext,
    /// The block hash of the base block,
    /// in which the virtual block is executed.
    pub(crate) base_block_hash: BlockHash,
    /// The commitment used for computing the block hash of the base block.
    pub(crate) base_block_header_commitments: BlockHeaderCommitments,
    /// The block hash of the previous base block.
    /// Used to compute the base block hash in the os.
    pub(crate) prev_base_block_hash: BlockHash,
}

impl BaseBlockInfo {
    /// Creates a `BaseBlockInfo` from a block header and chain info.
    ///
    /// When `use_latest_versioned_constants` is `true`, the latest versioned constants are used
    /// instead of the ones matching the block's Starknet version.
    pub(crate) fn new(
        header: BlockHeader,
        chain_info: ChainInfo,
        use_latest_versioned_constants: bool,
    ) -> Result<Self, VirtualBlockExecutorError> {
        let base_block_hash = header.block_hash;
        let prev_base_block_hash = header.parent_hash;
        let base_block_header_commitments = BlockHeaderCommitments {
            transaction_commitment: header.transaction_commitment,
            event_commitment: header.event_commitment,
            receipt_commitment: header.receipt_commitment,
            state_diff_commitment: header.state_diff_commitment,
            concatenated_counts: concat_counts(
                header.transaction_count,
                header.event_count,
                header.state_diff_length,
                header.l1_da_mode,
            ),
        };

        let block_info: BlockInfo = header.try_into().map_err(|e| {
            VirtualBlockExecutorError::TransactionExecutionError(format!(
                "Failed to convert block header to block info: {e}"
            ))
        })?;
        let mut versioned_constants = if use_latest_versioned_constants {
            VersionedConstants::latest_constants().clone()
        } else {
            VersionedConstants::get(&block_info.starknet_version)
                .map_err(|e| {
                    VirtualBlockExecutorError::TransactionExecutionError(format!(
                        "Failed to get versioned constants: {e}"
                    ))
                })?
                .clone()
        };
        // Disable casm hash migration for virtual block execution.
        versioned_constants.enable_casm_hash_migration = false;
        // Enable Sierra gas for all Cairo 1 contracts.
        // Version (0, 0, 0) is reserved for Cairo 0, so set to (0, 0, 1) for Cairo 1.
        versioned_constants.min_sierra_version_for_sierra_gas = SierraVersion::new(0, 0, 1);

        let block_context = BlockContext::new(
            block_info,
            chain_info,
            versioned_constants.clone(),
            BouncerConfig::default(),
        );

        Ok(BaseBlockInfo {
            block_context,
            base_block_hash,
            base_block_header_commitments,
            prev_base_block_hash,
        })
    }
}

pub(crate) struct VirtualBlockExecutionData {
    /// Execution outputs for all transactions in the virtual block.
    pub(crate) execution_outputs: Vec<TransactionExecutionOutput>,
    /// The initial state reads (accessed state) during execution.
    pub(crate) initial_reads: StateMaps,
    /// The state diff (changes made by transactions).
    pub(crate) state_diff: StateMaps,
    /// The class hashes of all contracts executed in the virtual block.
    pub(crate) executed_class_hashes: HashSet<ClassHash>,
    /// L2 to L1 messages.
    pub(crate) l2_to_l1_messages: Vec<MessageToL1>,
    /// The base block info for the virtual block.
    pub(crate) base_block_info: BaseBlockInfo,
}

/// Executes a virtual block of transactions.
///
/// A virtual block executor runs transactions on top of a given, finalized block.
///  This means that some parts, like block preprocessing
/// (`pre_process_block`), are skipped. Useful for simulating execution or generating
/// OS input for proving.
///
/// Implementations can fetch state from different sources (RPC nodes, local state,
/// mocked data, etc.).
///
/// # Note
///
/// - Currently only Invoke transactions are supported.
/// - Strict nonce check is always skipped.
/// - Fee charging is enabled when the transaction has non-zero resource bounds.
/// - Transaction validation is enabled by default.
///
/// # Examples
///
/// ```text
/// let executor = RpcVirtualBlockExecutor::new(
///     node_url: "http://localhost:9545".to_string(),
///     chain_id: ChainId::Mainnet,
///     block_id: BlockId::Number(BlockNumber(1000)),
/// );
/// let execution_data = executor.execute(block_id, contract_class_manager, transactions)?;
/// // Use execution_data to build OS input for proving...
/// ```
pub(crate) trait VirtualBlockExecutor: Send + 'static {
    /// Executes a virtual block based on the state and context at the given block ID.
    ///
    /// # Arguments
    ///
    /// * `block_id` - The block ID to use for state and context
    /// * `contract_class_manager` - Manager for compiled contract classes
    /// * `txs` - Invoke transactions to execute (with their hashes)
    ///
    /// # Returns
    ///
    /// Returns `VirtualBlockExecutionData` containing execution outputs for all
    /// transactions, or an error if any transaction fails.
    fn execute(
        &self,
        block_id: BlockId,
        contract_class_manager: ContractClassManager,
        txs: Vec<(InvokeTransaction, TransactionHash)>,
    ) -> Result<VirtualBlockExecutionData, VirtualBlockExecutorError> {
        let base_block_info = self.base_block_info(block_id)?;
        let state_reader = self.state_reader(block_id, &txs)?;
        let tx_hashes: Vec<TransactionHash> = txs.iter().map(|(_, h)| *h).collect();
        let blockifier_txs = self.convert_invoke_txs(txs)?;

        // Create state reader with contract manager.
        let state_reader_and_contract_manager =
            StateReaderAndContractManager::new(state_reader, contract_class_manager, None);

        let block_state = CachedState::new(state_reader_and_contract_manager);

        // Create executor WITHOUT preprocessing (no pre_process_block call).
        let mut transaction_executor = TransactionExecutor::new(
            block_state,
            base_block_info.block_context.clone(),
            TransactionExecutorConfig::default(),
        );

        // Execute all transactions.
        let execution_results = transaction_executor.execute_txs(&blockifier_txs, None);

        // Collect results, returning error if any transaction fails.
        let execution_outputs: Vec<TransactionExecutionOutput> = execution_results
            .into_iter()
            .map(|result| {
                result.map_err(|e| {
                    VirtualBlockExecutorError::TransactionExecutionError(e.to_string())
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Verify that all transactions were executed successfully (no reverted transactions).
        for (output, tx_hash) in execution_outputs.iter().zip(tx_hashes.iter()) {
            if let Some(revert_error) = &output.0.revert_error {
                return Err(VirtualBlockExecutorError::TransactionReverted(
                    *tx_hash,
                    revert_error.to_string(),
                ));
            }
        }

        let block_state = transaction_executor
            .block_state
            .as_mut()
            .ok_or(VirtualBlockExecutorError::StateUnavailable)?;
        // Get initial state reads.
        let initial_reads = block_state
            .get_initial_reads()
            .map_err(|e| VirtualBlockExecutorError::ReexecutionError(Box::new(e.into())))?;

        // Get state diff (changes made by transactions).
        let state_diff = block_state
            .to_state_diff()
            .map_err(|e| {
                VirtualBlockExecutorError::TransactionExecutionError(format!(
                    "Failed to get state diff: {e}"
                ))
            })?
            .state_maps;

        let executed_class_hashes = transaction_executor
            .bouncer
            .lock()
            .map_err(|e| {
                error!(
                    "Unexpected error: failed to acquire bouncer lock after transaction \
                     execution. This should never happen: {}",
                    e
                );
                VirtualBlockExecutorError::BouncerLockError(e.to_string())
            })?
            .get_executed_class_hashes();

        // Extract L2 to L1 messages.
        let mut l2_to_l1_messages = Vec::new();
        for (execution_info, _state_diff) in &execution_outputs {
            // This iterates through validate, execute, and fee_transfer call infos
            // and collects messages from all of them (including inner calls)
            let messages: Vec<MessageToL1> = execution_info
                .non_optional_call_infos()
                .flat_map(|call_info| call_info.get_sorted_l2_to_l1_messages())
                .collect();
            l2_to_l1_messages.extend(messages);
        }

        Ok(VirtualBlockExecutionData {
            execution_outputs,
            base_block_info,
            initial_reads,
            state_diff,
            l2_to_l1_messages,
            executed_class_hashes,
        })
    }

    /// Converts Invoke transactions to blockifier transactions.
    ///
    /// Uses execution flags that skip strict nonce check for virtual block execution.
    fn convert_invoke_txs(
        &self,
        txs: Vec<(InvokeTransaction, TransactionHash)>,
    ) -> Result<Vec<BlockifierTransaction>, VirtualBlockExecutorError> {
        txs.into_iter()
            .map(|(invoke_tx, tx_hash)| {
                // Execute with validation, conditional fee charging based on resource bounds,
                // but skip strict nonce check for virtual block execution.
                let execution_flags = ExecutionFlags {
                    only_query: false,
                    charge_fee: invoke_tx.resource_bounds().max_possible_fee(invoke_tx.tip())
                        > Fee(0),
                    validate: self.validate_txs_enabled()?,
                    strict_nonce_check: false,
                };

                BlockifierTransaction::from_api(
                    Transaction::Invoke(invoke_tx),
                    tx_hash,
                    None, // class_info - not needed for Invoke.
                    None, // paid_fee_on_l1 - not needed for Invoke.
                    None, // deployed_contract_address - not needed for Invoke.
                    execution_flags,
                )
                .map_err(|e| VirtualBlockExecutorError::TransactionExecutionError(e.to_string()))
            })
            .collect()
    }

    /// Returns the base block info for the given block ID.
    fn base_block_info(
        &self,
        block_id: BlockId,
    ) -> Result<BaseBlockInfo, VirtualBlockExecutorError>;

    /// Returns a state reader that implements `FetchCompiledClasses` for the given block ID.
    /// Must be `Send + Sync + 'static` to be used in the transaction executor.
    fn state_reader(
        &self,
        block_id: BlockId,
        txs: &[(InvokeTransaction, TransactionHash)],
    ) -> Result<Box<dyn FetchCompiledClasses + Send + Sync + 'static>, VirtualBlockExecutorError>;

    /// Returns whether transaction validation is enabled during execution.
    fn validate_txs_enabled(&self) -> Result<bool, VirtualBlockExecutorError>;
}

/// State reader backed by prefetched `StateMaps` from simulate.
///
/// Serves storage, nonce, class hash, and declared contract reads from the prefetched state.
/// Falls back to the inner `RpcStateReader` when a key is missing from the prefetched state
/// (e.g., when simulate uses different flags than execution).
/// Delegates compiled class lookups to the inner `RpcStateReader` since simulate responses
/// do not include classes.
#[allow(dead_code)]
pub(crate) struct SimulatedStateReader {
    state_maps: StateMaps,
    rpc_state_reader: RpcStateReader,
}

impl StateReader for SimulatedStateReader {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt> {
        match self.state_maps.storage.get(&(contract_address, key)) {
            Some(value) => Ok(*value),
            None => {
                warn!(
                    "Storage key not found in prefetched state, falling back to RPC \
                     (contract_address: {contract_address}, key: {key:?})."
                );
                self.rpc_state_reader.get_storage_at(contract_address, key)
            }
        }
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        match self.state_maps.nonces.get(&contract_address) {
            Some(value) => Ok(*value),
            None => {
                warn!(
                    "Nonce not found in prefetched state, falling back to RPC (contract_address: \
                     {contract_address})."
                );
                self.rpc_state_reader.get_nonce_at(contract_address)
            }
        }
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        match self.state_maps.class_hashes.get(&contract_address) {
            Some(value) => Ok(*value),
            None => {
                warn!(
                    "Class hash not found in prefetched state, falling back to RPC \
                     (contract_address: {contract_address})."
                );
                self.rpc_state_reader.get_class_hash_at(contract_address)
            }
        }
    }

    fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        self.rpc_state_reader.get_compiled_class(class_hash)
    }

    fn get_compiled_class_hash(&self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        self.rpc_state_reader.get_compiled_class_hash(class_hash)
    }

    fn get_compiled_class_hash_v2(
        &self,
        class_hash: ClassHash,
        compiled_class: &RunnableCompiledClass,
    ) -> StateResult<CompiledClassHash> {
        self.rpc_state_reader.get_compiled_class_hash_v2(class_hash, compiled_class)
    }
}

impl FetchCompiledClasses for SimulatedStateReader {
    fn get_compiled_classes(&self, class_hash: ClassHash) -> StateResult<CompiledClasses> {
        self.rpc_state_reader.get_compiled_classes(class_hash)
    }

    fn is_declared(&self, class_hash: ClassHash) -> StateResult<bool> {
        match self.state_maps.declared_contracts.get(&class_hash) {
            Some(value) => Ok(*value),
            None => {
                warn!(
                    "Declared contract not found in prefetched state, falling back to RPC \
                     (class_hash: {class_hash})."
                );
                self.rpc_state_reader.is_declared(class_hash)
            }
        }
    }
}

#[allow(dead_code)]
pub(crate) struct RpcVirtualBlockExecutor {
    /// The state reader for the virtual block executor.
    pub(crate) rpc_state_reader: RpcStateReader,
    /// Whether transaction validation is enabled during execution.
    pub(crate) validate_txs: bool,
    pub(crate) config: RpcVirtualBlockExecutorConfig,
}

impl RpcVirtualBlockExecutor {
    pub(crate) fn new(
        node_url: String,
        chain_info: ChainInfo,
        block_id: BlockId,
        config: RpcVirtualBlockExecutorConfig,
    ) -> Self {
        Self {
            rpc_state_reader: RpcStateReader::new_with_config_from_url(
                node_url, chain_info, block_id,
            ),
            validate_txs: true,
            config,
        }
    }

    /// Calls `starknet_simulateTransactions` with `RETURN_INITIAL_READS` and returns the
    /// initial state reads as `StateMaps`.
    ///
    /// Requires a v0.10+ node that supports the `RETURN_INITIAL_READS` flag.
    #[allow(dead_code)]
    pub(crate) fn simulate_and_get_initial_reads(
        &self,
        block_id: BlockId,
        txs: &[(InvokeTransaction, TransactionHash)],
    ) -> Result<StateMaps, VirtualBlockExecutorError> {
        let rpc_txs: Vec<RpcTransaction> = txs
            .iter()
            .map(|(tx, _)| match tx {
                InvokeTransaction::V3(v3) => RpcInvokeTransactionV3::try_from(v3.clone())
                    .map(RpcInvokeTransaction::V3)
                    .map(RpcTransaction::Invoke)
                    .map_err(|e: StarknetApiError| {
                        VirtualBlockExecutorError::TransactionExecutionError(e.to_string())
                    }),
                _ => Err(VirtualBlockExecutorError::TransactionExecutionError(
                    "Only Invoke V3 transactions are supported for simulate".to_string(),
                )),
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Build simulation flags that match execution behavior as closely as possible.
        // Mismatches cause prefetch cache misses (handled by RPC fallback) but hurt
        // performance.
        let mut simulation_flags = vec!["RETURN_INITIAL_READS"];
        if !self.validate_txs {
            simulation_flags.push("SKIP_VALIDATE");
        }
        // Fee charging during simulate can fail if the account lacks balance at the base
        // block. Skip it in simulate — fee-related storage keys will be fetched via RPC
        // fallback when needed.
        simulation_flags.push("SKIP_FEE_CHARGE");

        let params = json!({
            "block_id": block_id,
            "transactions": rpc_txs,
            "simulation_flags": simulation_flags
        });

        let result = self
            .rpc_state_reader
            .send_rpc_request("starknet_simulateTransactions", params)
            .map_err(|e| VirtualBlockExecutorError::ReexecutionError(Box::new(e.into())))?;

        let initial_reads_value = result.get("initial_reads").cloned().ok_or_else(|| {
            VirtualBlockExecutorError::TransactionExecutionError(
                "simulateTransactions response missing initial_reads (ensure RETURN_INITIAL_READS \
                 and v0.10 endpoint)"
                    .to_string(),
            )
        })?;

        deserialize_rpc_initial_reads(initial_reads_value).map_err(|e| {
            VirtualBlockExecutorError::TransactionExecutionError(format!(
                "Failed to deserialize initial_reads: {e}"
            ))
        })
    }
}

/// RPC-based virtual block executor.
///
/// This executor fetches historical state from an RPC node and executes transactions
/// without block preprocessing. Validation and fee charging are always skipped,
/// making it suitable for simulation and OS input generation.
impl VirtualBlockExecutor for RpcVirtualBlockExecutor {
    fn base_block_info(
        &self,
        _block_id: BlockId,
    ) -> Result<BaseBlockInfo, VirtualBlockExecutorError> {
        let block_header = self
            .rpc_state_reader
            .get_block_header()
            .map_err(|e| VirtualBlockExecutorError::ReexecutionError(Box::new(e)))?;
        let mut base_block_info = BaseBlockInfo::new(
            block_header,
            self.rpc_state_reader.chain_info.clone(),
            self.config.use_latest_versioned_constants,
        )?;

        // Client-side bouncer limits may differ from Starknet network limits.
        base_block_info.block_context.bouncer_config = self.config.bouncer_config.clone();

        Ok(base_block_info)
    }

    /// Returns a state reader that implements `FetchCompiledClasses` for the given block ID.
    /// When prefetching state, the state reader will be a `SimulatedStateReader` that uses the
    /// initial state reads to prefetch state, otherwise it will be the `RpcStateReader`.
    fn state_reader(
        &self,
        block_id: BlockId,
        txs: &[(InvokeTransaction, TransactionHash)],
    ) -> Result<Box<dyn FetchCompiledClasses + Send + Sync + 'static>, VirtualBlockExecutorError>
    {
        let rpc_state_reader = self.rpc_state_reader.clone();
        if self.config.prefetch_state {
            let state_maps = self.simulate_and_get_initial_reads(block_id, txs)?;
            Ok(Box::new(SimulatedStateReader { state_maps, rpc_state_reader }))
        } else {
            Ok(Box::new(rpc_state_reader))
        }
    }

    fn validate_txs_enabled(&self) -> Result<bool, VirtualBlockExecutorError> {
        Ok(self.validate_txs)
    }
}
