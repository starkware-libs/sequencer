use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::integration_test_manager::{
    IntegrationTestManager,
    BLOCK_TO_WAIT_FOR_DEPLOY_AND_INVOKE,
    DEFAULT_SENDER_ACCOUNT,
};
use apollo_integration_tests::integration_test_utils::integration_test_setup;
use apollo_integration_tests::utils::{load_proof_flow_genesis_params, ProofFlowTxs};
use starknet_api::block::BlockNumber;
use tracing::info;

#[tokio::main]
async fn main() {
    integration_test_setup("proof_flow").await;

    const N_CONSOLIDATED_SEQUENCERS: usize = 1;
    const N_DISTRIBUTED_SEQUENCERS: usize = 0;
    const N_HYBRID_SEQUENCERS: usize = 0;

    let genesis_params = load_proof_flow_genesis_params();
    let genesis_block = genesis_params.initial_block_number;

    let mut integration_test_manager = IntegrationTestManager::new(
        N_CONSOLIDATED_SEQUENCERS,
        N_DISTRIBUTED_SEQUENCERS,
        N_HYBRID_SEQUENCERS,
        None,
        TestIdentifier::ProofFlowIntegrationTest,
        genesis_params,
    )
    .await;

    let node_indices = integration_test_manager.get_node_indices();
    integration_test_manager.run_nodes(node_indices.clone()).await;

    // Deploy accounts and sync the tx_generator nonces with on-chain state.
    // Block numbers are absolute, so compute relative to genesis.
    let block_to_wait_for_deploy =
        BlockNumber(genesis_block.0 + BLOCK_TO_WAIT_FOR_DEPLOY_AND_INVOKE.0);
    integration_test_manager
        .send_deploy_and_invoke_txs_and_verify_at_block(block_to_wait_for_deploy)
        .await;

    // Send the proof-bearing transaction and wait for inclusion.
    // Wait 2 blocks beyond the deploy phase to give the proof-bearing tx time to be included.
    let block_to_wait_for_proof = BlockNumber(genesis_block.0 + 6);
    integration_test_manager
        .test_and_verify(ProofFlowTxs::new(), DEFAULT_SENDER_ACCOUNT, block_to_wait_for_proof)
        .await;

    info!("Shutting down nodes.");
    integration_test_manager.shutdown_nodes(node_indices);

    info!("Proof flow integration test completed successfully!");
}
