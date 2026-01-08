use std::sync::Arc;

use blockifier::blockifier::config::ContractClassManagerConfig;
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::state_reader_and_contract_manager::StateReaderAndContractManager;
use blockifier_reexecution::state_reader::rpc_state_reader::RpcStateReader;
use rstest::rstest;
use starknet_api::block::BlockNumber;

use crate::runner::Runner;
use crate::storage_proofs::RpcStorageProofsProvider;
use crate::test_utils::{
    latest_block_number,
    rpc_provider,
    rpc_state_reader,
    rpc_virtual_block_executor,
};
use crate::virtual_block_executor::RpcVirtualBlockExecutor;
use crate::virtual_block_executor_test::construct_balance_of_invoke_cairo1;

/// Integration test for Runner::run_os with a constructed transaction using Cairo 1 contracts.
///
/// This test:
/// 1. Fetches the latest block number dynamically from RPC (to ensure Cairo 1 contracts)
/// 2. Constructs a balanceOf call on the STRK token contract for Binance's balance
/// 3. Sets up a Runner with RPC-based providers
/// 4. Calls Runner::run_os to execute the transaction through the OS
/// 5. Verifies that the OS execution completes successfully
///
/// # Environment Variables
///
/// - `NODE_URL`: Required. URL of a Starknet mainnet RPC node.
///
/// # Running
///
/// ```bash
/// NODE_URL=https://your-rpc-node cargo test -p starknet_os_runner -- --ignored
/// ```
#[rstest]
#[tokio::test]
#[ignore] // Requires RPC access
async fn test_run_os_with_balance_of_transaction(
    #[future] latest_block_number: BlockNumber,
    #[future] rpc_state_reader: RpcStateReader,
    #[future] rpc_virtual_block_executor: RpcVirtualBlockExecutor,
    #[future] rpc_provider: RpcStorageProofsProvider,
    #[awt] latest_block_number: BlockNumber,
    #[awt] rpc_state_reader: RpcStateReader,
    #[awt] rpc_virtual_block_executor: RpcVirtualBlockExecutor,
    #[awt] rpc_provider: RpcStorageProofsProvider,
) {
    // Construct a balanceOf transaction querying Binance's balance in STRK token.
    // Pass rpc_state_reader to fetch the actual nonce at runtime.
    let (tx, tx_hash) = construct_balance_of_invoke_cairo1(&rpc_state_reader);

    // Create the contract class manager.
    let contract_class_manager = ContractClassManager::start(ContractClassManagerConfig::default());

    // Create StateReaderAndContractManager for classes provider.
    // Wrap in Arc to implement ClassesProvider trait.
    let classes_provider = Arc::new(StateReaderAndContractManager::new(
        rpc_state_reader.clone(),
        contract_class_manager.clone(),
        None,
    ));

    // Create the Runner with all three components.
    let runner = Runner::new(classes_provider, rpc_provider, rpc_virtual_block_executor);

    // Run the OS with the transaction using the latest block number.
    let result = runner
        .run_os(latest_block_number, contract_class_manager, vec![(tx, tx_hash)])
        .await
        .expect("run_os should complete successfully");

    // Verify that we got a result (basic sanity check).
    // The result contains cairo_pie, raw_output, metrics, etc.
    assert!(!result.raw_output.is_empty(), "OS output should not be empty");
    println!(
        "OS execution completed successfully at block {}. Output length: {}",
        latest_block_number.0,
        result.raw_output.len()
    );
}
