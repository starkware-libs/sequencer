//! Integration test for the proof submission flow.
//!
//! The test relies on pre-generated fixture files in
//! `crates/apollo_integration_tests/resources/proof_flow/`.

use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::integration_test_manager::{
    IntegrationTestManager,
    DEFAULT_SENDER_ACCOUNT,
};
use apollo_integration_tests::integration_test_utils::integration_test_setup;
use apollo_integration_tests::utils::{NodeDescriptor, ProofFlowTxs};
use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
use starknet_api::block::BlockNumber;
use tracing::info;

#[tokio::main]
async fn main() {
    integration_test_setup("proof_flow").await;

    // The fixture's proof references block 0, so the proof-bearing tx is only valid once the
    // chain has progressed past `STORED_BLOCK_HASH_BUFFER`. We first advance the chain past that
    // buffer with filler invokes, then submit the proof tx and wait one more block.
    const BLOCK_PAST_HASH_BUFFER: BlockNumber = BlockNumber(STORED_BLOCK_HASH_BUFFER);
    const BLOCK_TO_WAIT_FOR: BlockNumber = BlockNumber(STORED_BLOCK_HASH_BUFFER + 1);
    let node_descriptors = vec![
        NodeDescriptor::consolidated(),
        NodeDescriptor::distributed(),
        NodeDescriptor::hybrid(),
    ];

    let mut integration_test_manager = IntegrationTestManager::new(
        node_descriptors,
        None,
        TestIdentifier::ProofFlowIntegrationTest,
    )
    .await;

    let node_indices = integration_test_manager.get_node_indices();
    integration_test_manager.run_nodes(node_indices.clone()).await;

    integration_test_manager.send_deploy_and_invoke_txs_and_verify().await;

    integration_test_manager.await_block_on_all_running_nodes(BLOCK_PAST_HASH_BUFFER).await;

    integration_test_manager
        .test_and_verify(ProofFlowTxs::new(), DEFAULT_SENDER_ACCOUNT, BLOCK_TO_WAIT_FOR)
        .await;

    info!("Shutting down nodes.");
    integration_test_manager.shutdown_nodes(node_indices).await;

    info!("Proof flow integration test completed successfully!");
}
