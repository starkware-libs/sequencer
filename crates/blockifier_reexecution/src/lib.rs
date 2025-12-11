pub mod cli;
pub mod compile;
pub mod errors;
pub mod serde_utils;
pub mod state_reader;
pub mod utils;

use apollo_gateway_config::config::RpcStateReaderConfig;
use blockifier::blockifier::config::ContractClassManagerConfig;
use blockifier::blockifier::transaction_executor::TransactionExecutionOutput;
use blockifier::context::BlockContext;
use blockifier::state::cached_state::StateMaps;
use blockifier::state::contract_class_manager::ContractClassManager;
use errors::ReexecutionResult;
use starknet_api::block::BlockNumber;
use starknet_api::core::ChainId;
use starknet_api::transaction::Transaction;
use state_reader::rpc_state_reader::ConsecutiveRpcStateReaders;

/// Executes a single transaction at the given block number using the RPC state reader.
pub fn execute_single_transaction(
    block_number: BlockNumber,
    node_url: String,
    chain_id: ChainId,
    tx: Transaction,
) -> ReexecutionResult<(TransactionExecutionOutput, StateMaps, BlockContext)> {
    let rpc_state_reader_config = RpcStateReaderConfig::from_url(node_url);

    // Initialize the contract class manager.
    let mut contract_class_manager_config = ContractClassManagerConfig::default();
    if cfg!(feature = "cairo_native") {
        contract_class_manager_config.cairo_native_run_config.wait_on_native_compilation = true;
        contract_class_manager_config.cairo_native_run_config.run_cairo_native = true;
    }
    let contract_class_manager = ContractClassManager::start(contract_class_manager_config);

    // ConsecutiveRpcStateReaders expects the last constructed block number (previous block).
    assert!(block_number.0 != 0, "Cannot execute transaction at block 0");
    let prev_block_number = BlockNumber(block_number.0 - 1);

    let readers = ConsecutiveRpcStateReaders::new(
        prev_block_number,
        Some(rpc_state_reader_config),
        chain_id,
        false,
        contract_class_manager,
    );

    readers.execute_single_api_tx(tx)
}
