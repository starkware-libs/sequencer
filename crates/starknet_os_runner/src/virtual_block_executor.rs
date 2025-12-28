use std::collections::HashSet;

use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::blockifier::transaction_executor::{
    TransactionExecutionOutput,
    TransactionExecutor,
};
use blockifier::context::BlockContext;
use blockifier::state::cached_state::{CachedState, StateMaps};
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::state_reader_and_contract_manager::{
    FetchCompiledClasses,
    StateReaderAndContractManager,
};
use blockifier::transaction::account_transaction::ExecutionFlags;
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use blockifier_reexecution::state_reader::reexecution_state_reader::ReexecutionStateReader;
use blockifier_reexecution::state_reader::rpc_state_reader::RpcStateReader;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::{ChainId, ClassHash};
use starknet_api::transaction::fields::Fee;
use starknet_api::transaction::{InvokeTransaction, Transaction, TransactionHash};

use crate::errors::VirtualBlockExecutorError;

/// Captures execution data for a virtual block (multiple transactions).
///
/// A virtual block is a set of transactions executed together without block preprocessing,
/// useful for OS input generation and proving. This struct contains all the transaction execution
/// outputs, block context, and initial state reads needed for proof generation.
pub struct VirtualBlockExecutionData {
    /// Execution outputs for all transactions in the virtual block.
    pub execution_outputs: Vec<TransactionExecutionOutput>,
    /// The block context in which the transactions were executed.
    pub block_context: BlockContext,
    /// The initial state reads (accessed state) during execution.
    pub initial_reads: StateMaps,
    /// The class hashes of all contracts executed in the virtual block.
    pub executed_class_hashes: HashSet<ClassHash>,
    /// The block hash of the state at the start of the virtual block.
    pub prev_base_block_hash: BlockHash,
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
///     block_number: BlockNumber(1000),
/// );
/// let execution_data = executor.execute(block_number, contract_class_manager, transactions)?;
/// // Use execution_data to build OS input for proving...
/// ```
pub trait VirtualBlockExecutor {
    /// Executes a virtual block based on the state and context at the given block number.
    ///
    /// # Arguments
    ///
    /// * `block_number` - The block number to use for state and context
    /// * `contract_class_manager` - Manager for compiled contract classes
    /// * `txs` - Invoke transactions to execute (with their hashes)
    ///
    /// # Returns
    ///
    /// Returns `VirtualBlockExecutionData` containing execution outputs for all
    /// transactions, or an error if any transaction fails.
    fn execute(
        &self,
        block_number: BlockNumber,
        contract_class_manager: ContractClassManager,
        txs: Vec<(InvokeTransaction, TransactionHash)>,
    ) -> Result<VirtualBlockExecutionData, VirtualBlockExecutorError> {
        let blockifier_txs = self.convert_invoke_txs(txs)?;
        let block_context = self.block_context(block_number)?;
        let state_reader = self.state_reader(block_number)?;
        let prev_base_block_hash = self.prev_base_block_hash(block_number)?;

        // Create state reader with contract manager.
        let state_reader_and_contract_manager =
            StateReaderAndContractManager::new(state_reader, contract_class_manager, None);

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

        let executed_class_hashes = transaction_executor
            .bouncer
            .lock()
            .expect("Bouncer lock failed.")
            .get_executed_class_hashes();

        Ok(VirtualBlockExecutionData {
            execution_outputs,
            block_context,
            initial_reads,
            executed_class_hashes,
            prev_base_block_hash,
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
    /// Returns the block context for the given block number.
    fn block_context(
        &self,
        block_number: BlockNumber,
    ) -> Result<BlockContext, VirtualBlockExecutorError>;

    /// Returns the block hash of the state at the start of the virtual block.
    fn prev_base_block_hash(
        &self,
        block_number: BlockNumber,
    ) -> Result<BlockHash, VirtualBlockExecutorError>;

    /// Returns a state reader that implements `FetchCompiledClasses` for the given block number.
    /// Must be `Send + Sync + 'static` to be used in the transaction executor.
    fn state_reader(
        &self,
        block_number: BlockNumber,
    ) -> Result<impl FetchCompiledClasses + Send + Sync + 'static, VirtualBlockExecutorError>;

    /// Returns whether transaction validation is enabled during execution.
    fn validate_txs_enabled(&self) -> Result<bool, VirtualBlockExecutorError>;
}

#[allow(dead_code)]
pub(crate) struct RpcVirtualBlockExecutor {
    pub rpc_state_reader: RpcStateReader,
    /// Whether transaction validation is enabled during execution.
    pub validate_txs: bool,
}

impl RpcVirtualBlockExecutor {
    #[allow(dead_code)]
    pub fn new(node_url: String, chain_id: ChainId, block_number: BlockNumber) -> Self {
        Self {
            rpc_state_reader: RpcStateReader::new_with_config_from_url(
                node_url,
                chain_id,
                block_number,
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
    fn block_context(
        &self,
        _block_number: BlockNumber,
    ) -> Result<BlockContext, VirtualBlockExecutorError> {
        self.rpc_state_reader
            .get_block_context()
            .map_err(|e| VirtualBlockExecutorError::ReexecutionError(Box::new(e)))
    }

    fn prev_base_block_hash(
        &self,
        block_number: BlockNumber,
    ) -> Result<BlockHash, VirtualBlockExecutorError> {
        self.rpc_state_reader
            .get_old_block_hash(block_number)
            .map_err(|e| VirtualBlockExecutorError::ReexecutionError(Box::new(e)))
    }

    fn state_reader(
        &self,
        _block_number: BlockNumber,
    ) -> Result<impl FetchCompiledClasses + Send + Sync + 'static, VirtualBlockExecutorError> {
        // Clone the RpcStateReader to avoid lifetime issues ( not a big struct).
        Ok(self.rpc_state_reader.clone())
    }

    fn validate_txs_enabled(&self) -> Result<bool, VirtualBlockExecutorError> {
        Ok(self.validate_txs)
    }
}
