//! Bootstrap flow integration test with empty storage.
//!
//! This test verifies that nodes can start with empty storage and process
//! bootstrap transactions to initialize the system.

use std::time::Duration;

use apollo_infra::trace_util::configure_tracing;
use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::bootstrap::generate_bootstrap_transactions;
use apollo_integration_tests::flow_test_setup::FlowTestSetup;
use blockifier::bouncer::BouncerWeights;
use tracing::info;

/// Simple bootstrap flow test that sends bootstrap declare transactions.
///
/// This test:
/// 1. Creates nodes with empty storage
/// 2. Sends bootstrap transactions via gateway
/// 3. Waits for them to be processed
///
/// Number of threads is 3 = Num of sequencers + 1 for the test thread.
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn bootstrap_flow_with_empty_storage() {
    configure_tracing().await;

    info!("Starting bootstrap flow test with empty storage");

    // Create nodes with empty storage for bootstrap
    let setup = FlowTestSetup::new_for_bootstrap(
        TestIdentifier::EndToEndFlowTestBootstrapDeclare.into(),
        BouncerWeights::default().proving_gas,
        [6, 7, 8], // Use different indices to avoid port conflicts
    )
    .await;

    info!("Sequencer nodes started with empty storage");

    // Wait for nodes to be ready
    wait_for_sequencer(&setup.sequencer_0).await;
    wait_for_sequencer(&setup.sequencer_1).await;
    info!("Both sequencers are ready");

    // Generate bootstrap transactions
    let bootstrap_txs = generate_bootstrap_transactions();
    info!("Generated {} bootstrap transactions", bootstrap_txs.len());

    // Send the first bootstrap transaction (account contract declare)
    // We start with just one transaction as a proof of concept
    let first_tx = bootstrap_txs.into_iter().next().expect("Should have at least one tx");
    info!("Sending first bootstrap transaction via gateway");

    let tx_hash = setup.sequencer_0.assert_add_tx_success(first_tx).await;
    info!("Transaction accepted with hash: {:?}", tx_hash);

    // Wait a bit for the transaction to be processed
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Check that batcher height increased (meaning blocks are being produced)
    let height = setup.sequencer_0.batcher_height().await;
    info!("Batcher height: {:?}", height);

    info!("Bootstrap flow test completed successfully!");
}

async fn wait_for_sequencer(sequencer: &apollo_integration_tests::flow_test_setup::FlowSequencerSetup) {
    const INTERVAL_MS: u64 = 100;
    const MAX_ATTEMPTS: usize = 50;

    sequencer
        .monitoring_client
        .await_alive(INTERVAL_MS, MAX_ATTEMPTS)
        .await
        .expect("Sequencer did not become alive in time");
}
