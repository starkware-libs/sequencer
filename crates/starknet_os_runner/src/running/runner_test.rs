//! Integration tests for the Runner.

use blockifier::blockifier::config::ContractClassManagerConfig;
use blockifier::state::contract_class_manager::ContractClassManager;
use rstest::rstest;
use starknet_api::core::ChainId;
use url::Url;

use crate::running::runner::{RpcRunnerFactory, RunnerConfig, VirtualSnosRunner};
use crate::running::storage_proofs::StorageProofConfig;
use crate::running::test_utils::{
    fetch_sepolia_block_number,
    get_sepolia_rpc_url,
    privacy_pool_invoke,
    strk_balance_of_invoke,
    strk_transfer_invoke,
};

/// Integration test for the full Runner flow with a balance_of transaction.
///
/// Uses a dummy account on Sepolia that requires no signature validation.
/// This test verifies that the runner can successfully execute transactions and run the virtual OS
/// with state changes (nonce).
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

/// Integration test that runs a privacy pool transaction.
///
/// Uses a pre-signed privacy transaction that interacts with the privacy pool contract.
/// This test verifies that the runner can successfully execute privacy transactions.
///
/// # Running
///
/// ```bash
/// SEPOLIA_NODE_URL=https://your-rpc-node cargo test -p starknet_os_runner test_run_os_with_privacy_transaction -- --ignored
/// ```
#[rstest]
#[case(true)] // With state changes.
#[tokio::test(flavor = "multi_thread")]
#[ignore] // Requires RPC access.
async fn test_run_os_with_privacy_transaction(#[case] include_state_changes: bool) {
    // Create a custom factory with the specified run_committer setting.
    let rpc_url = get_sepolia_rpc_url();
    let rpc_url_parsed = Url::parse(&rpc_url).expect("Invalid Sepolia RPC URL");
    let contract_class_manager = ContractClassManager::start(ContractClassManagerConfig::default());
    let config =
        RunnerConfig { storage_proof_config: StorageProofConfig { include_state_changes } };
    let factory =
        RpcRunnerFactory::new(rpc_url_parsed, ChainId::Sepolia, contract_class_manager, config);

    let block_id = fetch_sepolia_block_number().await;
    let (tx, tx_hash) = privacy_pool_invoke();

    // Verify execution succeeds.
    factory
        .run_virtual_os(block_id, vec![(tx, tx_hash)])
        .await
        .expect("run_virtual_os should succeed");
}

/// Integration test for the full Runner flow with a STRK transfer transaction.
///
/// Uses the dummy account on Sepolia to transfer STRK to another address.
/// This test verifies that the runner can successfully execute state-changing
/// transactions (balance transfers) and run the virtual OS.
///
/// # Running
///
/// ```bash
/// SEPOLIA_NODE_URL=https://your-rpc-node cargo test -p starknet_os_runner test_run_os_with_transfer_transaction -- --ignored
/// ```
#[rstest]
#[case(true)] // With state changes.
#[tokio::test(flavor = "multi_thread")]
#[ignore] // Requires RPC access.
async fn test_run_os_with_transfer_transaction(#[case] include_state_changes: bool) {
    // Create a custom factory with the specified run_committer setting.
    let rpc_url = get_sepolia_rpc_url();
    let rpc_url_parsed = Url::parse(&rpc_url).expect("Invalid Sepolia RPC URL");
    let contract_class_manager = ContractClassManager::start(ContractClassManagerConfig::default());
    let config =
        RunnerConfig { storage_proof_config: StorageProofConfig { include_state_changes } };
    let factory =
        RpcRunnerFactory::new(rpc_url_parsed, ChainId::Sepolia, contract_class_manager, config);

    let block_id = fetch_sepolia_block_number().await;
    let (tx, tx_hash) = strk_transfer_invoke();

    // Verify execution succeeds.
    factory
        .run_virtual_os(block_id, vec![(tx, tx_hash)])
        .await
        .expect("run_virtual_os should succeed");
}
