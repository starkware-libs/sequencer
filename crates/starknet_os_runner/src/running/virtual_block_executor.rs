use std::collections::HashSet;

use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::blockifier::transaction_executor::{
    TransactionExecutionOutput,
    TransactionExecutor,
};
use blockifier::blockifier_versioned_constants::VersionedConstants;
use blockifier::bouncer::BouncerConfig;
use blockifier::context::BlockContext;
use blockifier::state::cached_state::{CachedState, StateMaps};
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::state_reader_and_contract_manager::{
    FetchCompiledClasses,
    StateReaderAndContractManager,
};
use blockifier::transaction::account_transaction::ExecutionFlags;
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use blockifier_reexecution::state_reader::rpc_objects::{BlockHeader, BlockId};
use blockifier_reexecution::state_reader::rpc_state_reader::RpcStateReader;
use blockifier_reexecution::utils::get_chain_info;
use starknet_api::block::{BlockHash, BlockInfo};
use starknet_api::block_hash::block_hash_calculator::{concat_counts, BlockHeaderCommitments};
use starknet_api::contract_class::SierraVersion;
use starknet_api::core::{ChainId, ClassHash};
use starknet_api::transaction::fields::Fee;
use starknet_api::transaction::{InvokeTransaction, MessageToL1, Transaction, TransactionHash};
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;
use tracing::error;

use crate::errors::VirtualBlockExecutorError;

/// Captures execution data for a virtual block (multiple transactions).
///
/// This struct contains all the execution data needed for proof generation.
#[allow(dead_code)]
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

impl TryFrom<(BlockHeader, ChainId)> for BaseBlockInfo {
    type Error = VirtualBlockExecutorError;

    fn try_from((header, chain_id): (BlockHeader, ChainId)) -> Result<Self, Self::Error> {
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
        let chain_info = get_chain_info(&chain_id);
        let mut versioned_constants = VersionedConstants::get(&block_info.starknet_version)
            .map_err(|e| {
                VirtualBlockExecutorError::TransactionExecutionError(format!(
                    "Failed to get versioned constants: {e}"
                ))
            })?
            .clone();
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

#[allow(dead_code)]
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
/// - fee charging and nonce check are always skipped (useful for simulation/proving).
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
#[allow(dead_code)]
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
        let _tx_hashes: Vec<TransactionHash> = txs.iter().map(|(_, h)| *h).collect();
        let blockifier_txs = self.convert_invoke_txs(txs)?;
        let base_block_info = self.base_block_info(block_id)?;
        let state_reader = self.state_reader(block_id)?;

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
    ) -> Result<impl FetchCompiledClasses + Send + Sync + 'static, VirtualBlockExecutorError>;

    /// Returns whether transaction validation is enabled during execution.
    fn validate_txs_enabled(&self) -> Result<bool, VirtualBlockExecutorError>;
}

#[allow(dead_code)]
pub(crate) struct RpcVirtualBlockExecutor {
    /// The state reader for the virtual block executor.
    pub(crate) rpc_state_reader: RpcStateReader,
    /// Whether transaction validation is enabled during execution.
    pub(crate) validate_txs: bool,
}

#[allow(dead_code)]
impl RpcVirtualBlockExecutor {
    pub(crate) fn new(node_url: String, chain_id: ChainId, block_id: BlockId) -> Self {
        Self {
            rpc_state_reader: RpcStateReader::new_with_config_from_url(
                node_url, chain_id, block_id,
            ),
            validate_txs: true,
        }
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
        BaseBlockInfo::try_from((block_header, self.rpc_state_reader.chain_id.clone()))
    }

    fn state_reader(
        &self,
        _block_id: BlockId,
    ) -> Result<impl FetchCompiledClasses + Send + Sync + 'static, VirtualBlockExecutorError> {
        // Clone the RpcStateReader to avoid lifetime issues ( not a big struct).
        Ok(self.rpc_state_reader.clone())
    }

    fn validate_txs_enabled(&self) -> Result<bool, VirtualBlockExecutorError> {
        Ok(self.validate_txs)
    }
}
