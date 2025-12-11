use blockifier::blockifier::transaction_executor::TransactionExecutionOutput;
use blockifier::context::BlockContext;
use blockifier::state::cached_state::StateMaps;
use blockifier_reexecution::execute_single_transaction;
use starknet_api::block::BlockNumber;
use starknet_api::core::ChainId;
use starknet_api::transaction::Transaction;

use crate::errors::ExecutionDataError;

/// Captures all execution data from blockifier needed for OS input generation.
///
/// This struct contains the complete execution state and context information that blockifier
/// produces when executing a transaction. It serves as the foundation for generating the OS
/// (Operating System) input required for proof generation.
pub struct BlockifierExecutionData {
    /// The output from executing the transaction, including execution info and state changes.
    pub execution_output: TransactionExecutionOutput,
    /// The block context in which the transaction was executed.
    pub block_context: BlockContext,
    /// The initial state reads (accessed state) during transaction execution.
    pub initial_reads: StateMaps,
}

/// Provides execution data from various sources for OS input generation.
///
/// This trait abstracts over different ways of obtaining transaction execution data from
/// blockifier. Implementations can fetch execution data from different sources (RPC nodes,
/// local state, mocked data, etc.).
///
/// The primary use case is the first step in OS input generation: capturing the necessary
/// execution information from blockifier that will be used to construct the full OS input
/// for proving.
///
/// # Examples
///
/// ```ignore
/// let provider = RpcExecutionDataProvider {
///     rpc_url: "http://localhost:9545".to_string(),
///     chain_id: ChainId::Mainnet,
/// };
///
/// let execution_data = provider.get_execution_data(block_number, transaction)?;
/// // Use execution_data to build OS input...
/// ```
pub trait ExecutionDataProvider {
    /// Retrieves execution data for a transaction at a specific block.
    ///
    /// # Arguments
    ///
    /// * `block_number` - The block number at which to execute the transaction
    /// * `tx` - The transaction to execute
    ///
    /// # Returns
    ///
    /// Returns `BlockifierExecutionData` containing all execution information needed for OS
    /// input generation, or an error if execution fails.
    fn get_execution_data(
        &self,
        block_number: BlockNumber,
        tx: Transaction,
    ) -> Result<BlockifierExecutionData, ExecutionDataError>;
}

/// RPC-based implementation of `ExecutionDataProvider`.
///
/// This provider fetches historical state from an RPC node and uses blockifier's re-execution
/// functionality to obtain execution data.
pub struct RpcExecutionDataProvider {
    /// URL of the RPC node to fetch state from.
    pub rpc_url: String,
    /// The chain ID for which to execute transactions.
    pub chain_id: ChainId,
}

impl ExecutionDataProvider for RpcExecutionDataProvider {
    fn get_execution_data(
        &self,
        block_number: BlockNumber,
        tx: Transaction,
    ) -> Result<BlockifierExecutionData, ExecutionDataError> {
        let (execution_output, initial_reads, block_context) = execute_single_transaction(
            block_number,
            self.rpc_url.clone(),
            self.chain_id.clone(),
            tx,
        )
        .map_err(|e| ExecutionDataError::from(Box::new(e)))?;

        Ok(BlockifierExecutionData { execution_output, block_context, initial_reads })
    }
}
