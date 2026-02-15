//! Integration tests for the Runner.

use blockifier::blockifier::config::ContractClassManagerConfig;
use blockifier::state::contract_class_manager::ContractClassManager;
use rstest::rstest;
use starknet_api::core::ChainId;
use url::Url;

use crate::runner::{RpcRunnerFactory, RunnerConfig, VirtualSnosRunner};
use crate::storage_proofs::StorageProofConfig;
use crate::test_utils::{
    fetch_privacy_block_number, fetch_sepolia_block_number, get_privacy_rpc_url,
    get_sepolia_rpc_url, privacy_invoke_tx, strk_balance_of_invoke,
};

/// Integration test for the full Runner flow with a balance_of transaction.
///
/// Uses a dummy account on Sepolia that requires no signature validation.
/// This test verifies that the runner can successfully execute transactions and run the cirtual OS
/// with state changes ( nounce ).
///
/// # Running
///
/// ```bash
/// SEPOLIA_NODE_URL=https://your-rpc-node cargo test -p starknet_os_runner test_run_os_with_balance_of_transaction -- --ignored
/// ```
#[rstest]
#[case(true)] // With state changes.
#[tokio::test(flavor = "multi_thread")]
#[ignore] // Requires RPC access.
async fn test_run_os_with_balance_of_transaction(#[case] include_state_changes: bool) {
    // Create a custom factory with the specified run_committer setting.
    let rpc_url = get_sepolia_rpc_url();
    let rpc_url_parsed = Url::parse(&rpc_url).expect("Invalid Sepolia RPC URL");
    let contract_class_manager = ContractClassManager::start(ContractClassManagerConfig::default());
    let config =
        RunnerConfig { storage_proof_config: StorageProofConfig { include_state_changes } };
    let factory =
        RpcRunnerFactory::new(rpc_url_parsed, ChainId::Sepolia, contract_class_manager, config);

    let block_id = fetch_sepolia_block_number().await;
    let (tx, tx_hash) = strk_balance_of_invoke();

    // Verify execution succeeds.
    factory
        .run_virtual_os(block_id, vec![(tx, tx_hash)])
        .await
        .expect("run_virtual_os should succeed");
}

/// Integration test for the full Runner flow with a privacy transaction.
///
/// Uses the privacy-starknet-pathfinder node.
///
/// # Running
///
/// ```bash
/// PRIVACY_NODE_URL=http://localhost:9547/rpc/v0_10 cargo test -p starknet_os_runner test_run_os_with_privacy_transaction -- --ignored
/// ```
#[rstest]
#[case(true)] // With state changes.
#[tokio::test(flavor = "multi_thread")]
#[ignore] // Requires RPC access.
async fn test_run_os_with_privacy_transaction(#[case] include_state_changes: bool) {
    // Create a custom factory with the specified run_committer setting.
    let rpc_url = get_privacy_rpc_url();
    let rpc_url_parsed = Url::parse(&rpc_url).expect("Invalid Privacy RPC URL");
    let contract_class_manager = ContractClassManager::start(ContractClassManagerConfig::default());
    let config =
        RunnerConfig { storage_proof_config: StorageProofConfig { include_state_changes } };
    // Use Sepolia chain ID for infrastructure compatibility, but privacy_invoke_tx()
    // calculates the tx hash with the correct chain ID for the privacy network.
    let factory =
        RpcRunnerFactory::new(rpc_url_parsed, ChainId::Sepolia, contract_class_manager, config);

    let block_id = fetch_privacy_block_number().await;
    let (tx, tx_hash) = privacy_invoke_tx();

    println!("Transaction hash: {:?}", tx_hash);
    println!("Block ID: {:?}", block_id);

    // Verify execution succeeds.
    let _result = factory
        .run_virtual_os(block_id, vec![(tx, tx_hash)])
        .await
        .expect("run_virtual_os should succeed");

    println!("Virtual OS execution completed successfully");
}
