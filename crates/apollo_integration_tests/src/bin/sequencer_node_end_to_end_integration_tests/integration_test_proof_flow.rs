//! Integration test for the proof submission flow.
//!
//! The test relies on pre-generated fixture files in
//! `crates/apollo_integration_tests/resources/proof_flow/`. Run
//! `./scripts/generate_proof_flow_fixtures.sh` to regenerate them whenever they become stale.

use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::integration_test_manager::{
    IntegrationTestManager,
    BLOCK_TO_WAIT_FOR_DEPLOY_AND_INVOKE,
    DEFAULT_SENDER_ACCOUNT,
};
use apollo_integration_tests::integration_test_utils::integration_test_setup;
use apollo_integration_tests::utils::{load_proof_flow_snos_facts, ConsensusTxs, ProofFlowTxs};
use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
use starknet_api::block::BlockNumber;
use tracing::info;

#[tokio::main]
async fn main() {
    integration_test_setup("proof_flow").await;

    const N_CONSOLIDATED_SEQUENCERS: usize = 2;
    const N_DISTRIBUTED_SEQUENCERS: usize = 0;
    const N_HYBRID_SEQUENCERS: usize = 0;

    let mut integration_test_manager = IntegrationTestManager::new(
        N_CONSOLIDATED_SEQUENCERS,
        N_DISTRIBUTED_SEQUENCERS,
        N_HYBRID_SEQUENCERS,
        None,
        TestIdentifier::ProofFlowIntegrationTest,
    )
    .await;

    let node_indices = integration_test_manager.get_node_indices();
    integration_test_manager.run_nodes(node_indices.clone()).await;

    let snos_facts = load_proof_flow_snos_facts();
    let proof_block_number = snos_facts.block_number;
    let block_hash_available_at = BlockNumber(proof_block_number.0 + STORED_BLOCK_HASH_BUFFER + 1);
    let wait_block = block_hash_available_at.max(BLOCK_TO_WAIT_FOR_DEPLOY_AND_INVOKE);
    integration_test_manager
        .test_and_verify(
            ConsensusTxs { n_invoke_txs: 1, n_l1_handler_txs: 0 },
            DEFAULT_SENDER_ACCOUNT,
            wait_block,
        )
        .await;

    // The proof references block 0. Wait long enough that block 0's hash is available before
    // sending the proof-bearing txs.
    let block_to_wait_for_proof = BlockNumber(proof_block_number.0 + STORED_BLOCK_HASH_BUFFER + 2);

    integration_test_manager
        .test_and_verify(ProofFlowTxs::new(), DEFAULT_SENDER_ACCOUNT, block_to_wait_for_proof)
        .await;

    info!("Shutting down nodes.");
    integration_test_manager.shutdown_nodes(node_indices);

    info!("Proof flow integration test completed successfully!");
}
