//! Bootstrap System Flow Test
//!
//! This test demonstrates bootstrapping a system from a completely empty state.
//! The system starts with no genesis block, no accounts, no contracts - truly empty storage.
//! Bootstrap transactions are used to initialize the system from scratch.

use std::fs::File;
use std::path::PathBuf;

use apollo_infra_utils::test_utils::TestIdentifier;
use starknet_api::execution_resources::GasAmount;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;

use crate::common::{end_to_end_flow, EndToEndFlowArgs, TestScenario};

mod common;

/// Loads the bootstrap declare transaction from the JSON file.
fn load_bootstrap_declare_tx() -> RpcTransaction {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let json_path = manifest_dir.join("tests/test_data/bootstrap_declare.json");

    let file = File::open(&json_path)
        .unwrap_or_else(|e| panic!("Failed to open {}: {}", json_path.display(), e));
    serde_json::from_reader(file)
        .unwrap_or_else(|e| panic!("Failed to parse {}: {}", json_path.display(), e))
}

/// Creates test scenarios for bootstrapping the system.
/// Each scenario sends bootstrap transactions to declare and deploy contracts.
fn create_bootstrap_scenarios() -> Vec<TestScenario> {
    vec![
        // First scenario: Send a bootstrap declare transaction
        TestScenario {
            // Load the bootstrap declare transaction from JSON file
            create_rpc_txs_fn: |_tx_generator| vec![load_bootstrap_declare_tx()],
            create_l1_to_l2_messages_args_fn: |_| vec![],
            test_tx_hashes_fn: test_bootstrap_tx,
        },
        // TODO: Add more scenarios for:
        // - Declaring fee token contracts
        // - Deploying fee token contracts
        // - Declaring account contracts
        // - Deploying and funding initial accounts
    ]
}

fn test_bootstrap_tx(tx_hashes: &[TransactionHash]) -> Vec<TransactionHash> {
    assert!(!tx_hashes.is_empty(), "Expected at least one bootstrap transaction");
    tx_hashes.to_vec()
}

/// Bootstrap system flow test.
///
/// This test:
/// 1. Starts sequencer nodes with COMPLETELY EMPTY storage (no genesis, no accounts)
/// 2. Sends bootstrap transactions to initialize the system
/// 3. Verifies transactions are processed by checking the BATCHED_TRANSACTIONS metric
///
/// The test uses 3 threads: 1 for the test's main thread and 2 for the sequencers.
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn test_bootstrap_system_flow() {
    end_to_end_flow(
        EndToEndFlowArgs::new(
            TestIdentifier::BootstrapSystemFlowTest,
            create_bootstrap_scenarios(),
            GasAmount(100_000_000), // Sufficient gas for bootstrap transactions
        )
        .allow_bootstrap_txs() // Required for bootstrap address transactions
        .empty_storage(), // Start with completely empty storage!
    )
    .await
}
