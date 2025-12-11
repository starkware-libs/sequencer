use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::blockifier::transaction_executor::{
    TransactionExecutionOutput,
    TransactionExecutor,
};
use blockifier::context::BlockContext;
use blockifier::state::cached_state::{CachedState, StateMaps};
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::state_reader_and_contract_manager::StateReaderAndContractManager;
use blockifier::transaction::account_transaction::ExecutionFlags;
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use blockifier_reexecution::state_reader::rpc_state_reader::RpcStateReader;
use starknet_api::block::BlockNumber;
use starknet_api::core::ChainId;
use starknet_api::transaction::{Transaction, TransactionHash};

use crate::errors::VirtualBlockExecutorError;

/// Captures execution data for a virtual block (multiple transactions).
///
/// A virtual block is a set of transactions executed together without block preprocessing,
/// useful for OS input generation and proving. This struct contains all the execution
/// outputs, block context, and initial state reads needed for proof generation.
pub struct VirtualBlockExecutionData {
    /// Execution outputs for all transactions in the virtual block.
    pub execution_outputs: Vec<TransactionExecutionOutput>,
    /// The block context in which the transactions were executed.
    pub block_context: BlockContext,
    /// The initial state reads (accessed state) during execution.
    pub initial_reads: StateMaps,
}

/// Executes a virtual block of transactions.
///
/// A virtual block executor runs transactions without block preprocessing
/// (`pre_process_block`), which is useful for simulating execution or generating
/// OS input for proving.
///
/// Implementations can fetch state from different sources (RPC nodes, local state,
/// mocked data, etc.).
///
/// # Note
///
/// Currently only Invoke transactions are supported.
///
/// # Examples
///
/// ```ignore
/// let executor = RpcVirtualBlockExecutor::new(
///     "http://localhost:9545".to_string(),
///     ChainId::Mainnet,
///     contract_class_manager,
/// );
///
/// let execution_data = executor.execute(block_number, transactions)?;
/// // Use execution_data to build OS input for proving...
/// ```
pub trait VirtualBlockExecutor {
    /// Executes a virtual block at the given block number.
    ///
    /// # Arguments
    ///
    /// * `block_number` - The block number to use for state and context
    /// * `txs` - Invoke transactions to execute (with their hashes)
    ///
    /// # Returns
    ///
    /// Returns `VirtualBlockExecutionData` containing execution outputs for all
    /// transactions, or an error if any transaction fails or is not an Invoke.
    fn execute(
        &self,
        block_number: BlockNumber,
        txs: Vec<(Transaction, TransactionHash)>,
    ) -> Result<VirtualBlockExecutionData, VirtualBlockExecutorError>;
}

/// RPC-based virtual block executor.
///
/// This executor fetches historical state from an RPC node and executes transactions
/// without block preprocessing. Useful for OS input generation and proving.
pub struct RpcVirtualBlockExecutor {
    node_url: String,
    chain_id: ChainId,
    contract_class_manager: ContractClassManager,
}

impl RpcVirtualBlockExecutor {
    /// Creates a new RPC-based virtual block executor.
    ///
    /// # Arguments
    ///
    /// * `node_url` - URL of the RPC node to fetch state from
    /// * `chain_id` - The chain ID for transaction hash computation
    /// * `contract_class_manager` - Manager for compiled contract classes
    pub fn new(
        node_url: String,
        chain_id: ChainId,
        contract_class_manager: ContractClassManager,
    ) -> Self {
        Self { node_url, chain_id, contract_class_manager }
    }

    /// Converts Invoke transactions to blockifier transactions.
    ///
    /// Returns an error if any transaction is not an Invoke.
    fn convert_invoke_txs(
        txs: Vec<(Transaction, TransactionHash)>,
    ) -> Result<Vec<BlockifierTransaction>, VirtualBlockExecutorError> {
        let execution_flags = ExecutionFlags::default();

        txs.into_iter()
            .map(|(tx, tx_hash)| {
                if !matches!(tx, Transaction::Invoke(_)) {
                    return Err(VirtualBlockExecutorError::UnsupportedTransactionType);
                }

                BlockifierTransaction::from_api(
                    tx,
                    tx_hash,
                    None, // class_info - not needed for Invoke.
                    None, // paid_fee_on_l1 - not needed for Invoke.
                    None, // deployed_contract_address - not needed for Invoke.
                    execution_flags.clone(),
                )
                .map_err(|e| VirtualBlockExecutorError::TransactionExecutionError(e.to_string()))
            })
            .collect()
    }
}

impl VirtualBlockExecutor for RpcVirtualBlockExecutor {
    fn execute(
        &self,
        block_number: BlockNumber,
        txs: Vec<(Transaction, TransactionHash)>,
    ) -> Result<VirtualBlockExecutionData, VirtualBlockExecutorError> {
        // Create RPC state reader for the given block.
        let rpc_state_reader = RpcStateReader::new_with_default_config(
            self.node_url.clone(),
            self.chain_id.clone(),
            block_number,
        );

        // Get block context from RPC.
        let block_context = rpc_state_reader
            .get_block_context()
            .map_err(|e| VirtualBlockExecutorError::ReexecutionError(Box::new(e)))?;

        // Convert Invoke transactions to blockifier transactions.
        let blockifier_txs = Self::convert_invoke_txs(txs)?;

        // Create state reader with contract manager.
        let state_reader_and_contract_manager = StateReaderAndContractManager::new(
            rpc_state_reader,
            self.contract_class_manager.clone(),
            None,
        );

        let block_state = CachedState::new(state_reader_and_contract_manager);

        // Create executor WITHOUT preprocessing (no pre_process_block call).
        let mut transaction_executor = TransactionExecutor::new(
            block_state,
            block_context.clone(),
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

        // Get initial state reads.
        let initial_reads = transaction_executor
            .block_state
            .as_ref()
            .ok_or(VirtualBlockExecutorError::StateUnavailable)?
            .get_initial_reads()
            .map_err(|e| VirtualBlockExecutorError::ReexecutionError(Box::new(e.into())))?;

        Ok(VirtualBlockExecutionData { execution_outputs, block_context, initial_reads })
    }
}
