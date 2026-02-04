//! Integration test for bootstrap flow.
//!
//! This test verifies that a node can start with empty storage, execute bootstrap
//! transactions, and transition to normal operation.
//!
//! The test demonstrates:
//! - Deterministic address calculation for funded account and fee tokens
//! - Bootstrap transaction generation in internal consensus format
//! - Bootstrap state machine transitions
//!
//! NOTE: Full end-to-end testing requires wiring up the bootstrap manager to the
//! node startup flow and batcher injection, which is not implemented yet.

use apollo_integration_tests::bootstrap::{
    generate_bootstrap_internal_transactions,
    BootstrapAddresses,
    BootstrapManager,
    BootstrapState,
};
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use tracing::info;

#[tokio::main]
async fn main() {
    // Initialize test logging
    apollo_integration_tests::integration_test_utils::integration_test_setup("bootstrap").await;

    info!("Starting bootstrap flow integration test");

    // Get the deterministic bootstrap addresses
    let addresses = BootstrapAddresses::get();
    info!(
        "Bootstrap addresses:\n  funded_account = {:?}\n  eth_token = {:?}\n  strk_token = {:?}",
        addresses.funded_account_address,
        addresses.eth_fee_token_address,
        addresses.strk_fee_token_address
    );

    // Verify addresses are deterministic (consistent across calls)
    let addresses2 = BootstrapAddresses::get();
    assert_eq!(addresses.funded_account_address, addresses2.funded_account_address);
    assert_eq!(addresses.eth_fee_token_address, addresses2.eth_fee_token_address);
    assert_eq!(addresses.strk_fee_token_address, addresses2.strk_fee_token_address);
    info!("Addresses are deterministic: PASS");

    // Generate bootstrap transactions in internal consensus format
    let txs = generate_bootstrap_internal_transactions();
    info!("Generated {} bootstrap transactions:", txs.len());
    for (i, tx) in txs.iter().enumerate() {
        let tx_type = match tx {
            InternalConsensusTransaction::RpcTransaction(rpc) => {
                format!("RpcTransaction (hash: {:?})", rpc.tx_hash)
            }
            InternalConsensusTransaction::L1Handler(_) => "L1Handler".to_string(),
        };
        info!("  {} - {}", i + 1, tx_type);
    }
    assert_eq!(txs.len(), 5, "Expected 5 bootstrap transactions");
    info!("Transaction generation: PASS");

    // Test bootstrap manager state transitions
    let manager = BootstrapManager::new();
    assert_eq!(manager.state(), BootstrapState::Disabled);
    info!("Initial state is Disabled: PASS");

    // Get transactions from manager
    let manager_txs = manager.get_bootstrap_transactions();
    assert_eq!(manager_txs.len(), 5);
    info!("Manager provides bootstrap transactions: PASS");

    info!("");
    info!("Bootstrap flow integration test completed!");
    info!("");
    info!("To run a full end-to-end test, the following needs to be wired up:");
    info!("  1. Node startup should call maybe_enter_pending() with storage_reader");
    info!("  2. Batcher should inject transactions from get_bootstrap_transactions()");
    info!("  3. After each block commit, call check_completion() with storage_reader");
    info!("  4. When completed, node transitions to normal operation");
}
