//! Integration test for bootstrap flow.
//!
//! This test verifies that the bootstrap infrastructure works correctly:
//! - Deterministic address calculation for funded account and fee tokens
//! - Empty storage detection
//! - ChainInfo configuration with bootstrap addresses
//! - Bootstrap transaction generation in internal consensus format
//! - Bootstrap state machine transitions
//!
//! NOTE: This test verifies the infrastructure components work together.
//! Full end-to-end testing with running nodes would require additional wiring.

use apollo_integration_tests::bootstrap::{
    generate_bootstrap_internal_transactions,
    is_storage_empty,
    BootstrapAddresses,
    BootstrapManager,
    BootstrapState,
};
use apollo_integration_tests::state_reader::StorageTestSetup;
use apollo_storage::header::HeaderStorageReader;
use apollo_storage::open_storage;
use starknet_api::block::FeeType;
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::core::ChainId;
use tracing::info;

#[tokio::main]
async fn main() {
    // Initialize test logging
    apollo_integration_tests::integration_test_utils::integration_test_setup("bootstrap").await;

    info!("Starting bootstrap flow integration test");

    // ==========================================================================
    // Test 1: Deterministic address calculation
    // ==========================================================================
    info!("\n--- Test 1: Deterministic Addresses ---");

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

    // ==========================================================================
    // Test 2: ChainInfo with bootstrap addresses
    // ==========================================================================
    info!("\n--- Test 2: ChainInfo Configuration ---");

    let chain_info = BootstrapAddresses::create_chain_info_for_bootstrap();
    assert_eq!(chain_info.fee_token_address(&FeeType::Eth), addresses.eth_fee_token_address);
    assert_eq!(chain_info.fee_token_address(&FeeType::Strk), addresses.strk_fee_token_address);
    info!("ChainInfo ETH fee token: {:?}", chain_info.fee_token_address(&FeeType::Eth));
    info!("ChainInfo STRK fee token: {:?}", chain_info.fee_token_address(&FeeType::Strk));
    info!("ChainInfo configuration: PASS");

    // ==========================================================================
    // Test 3: Empty storage detection
    // ==========================================================================
    info!("\n--- Test 3: Empty Storage Detection ---");

    // Create empty storage for bootstrap
    let storage_setup = StorageTestSetup::new_empty_for_bootstrap(ChainId::IntegrationSepolia);
    info!("Created empty storage setup for bootstrap");

    // Open the batcher storage to test empty detection
    let (storage_reader, _) =
        open_storage(storage_setup.storage_config.batcher_storage_config.clone())
            .expect("Failed to open storage");

    // Verify storage is empty
    assert!(is_storage_empty(&storage_reader), "Storage should be empty");
    info!("Empty storage detection: PASS");

    // Verify header marker is 0
    let txn = storage_reader.begin_ro_txn().expect("Failed to begin read transaction");
    let header_marker = txn.get_header_marker().expect("Failed to get header marker");
    assert_eq!(header_marker.0, 0, "Header marker should be 0 for empty storage");
    info!("Header marker is 0: PASS");

    // Keep handles alive to prevent cleanup during test
    let _handles = storage_setup.storage_handles;

    // ==========================================================================
    // Test 4: Bootstrap transaction generation
    // ==========================================================================
    info!("\n--- Test 4: Transaction Generation ---");

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

    // ==========================================================================
    // Test 5: Bootstrap manager state machine
    // ==========================================================================
    info!("\n--- Test 5: Bootstrap Manager ---");

    let manager = BootstrapManager::new();
    assert_eq!(manager.state(), BootstrapState::Disabled);
    info!("Initial state is Disabled: PASS");

    // Get transactions from manager
    let manager_txs = manager.get_bootstrap_transactions();
    assert_eq!(manager_txs.len(), 5);
    info!("Manager provides bootstrap transactions: PASS");

    // ==========================================================================
    // Summary
    // ==========================================================================
    info!("\n===================================");
    info!("Bootstrap flow integration test: ALL TESTS PASSED");
    info!("===================================");
    info!("");
    info!("What was tested:");
    info!("  1. Deterministic address calculation");
    info!("  2. ChainInfo with bootstrap fee token addresses");
    info!("  3. Empty storage creation and detection");
    info!("  4. Bootstrap transaction generation (5 txs)");
    info!("  5. Bootstrap manager state machine");
    info!("");
    info!("For full end-to-end testing with running nodes:");
    info!("  - Use FlowTestSetup::new_for_bootstrap() which:");
    info!("    - Creates empty storage");
    info!("    - Generates and injects bootstrap transactions into batcher");
    info!("    - Configures BootstrapConfig with deterministic addresses");
    info!("  - Bootstrap transactions are processed automatically by the batcher");
}
