use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::block::BlockNumber;
use starknet_api::transaction::TransactionHash;
use starknet_batcher_types::batcher_types::{
    BuildProposalInput,
    GetProposalContent,
    GetProposalContentInput,
    ProposalId,
    StartHeightInput,
};
use starknet_batcher_types::communication::SharedBatcherClient;
use starknet_integration_tests::integration_test_setup::IntegrationTestSetup;
use starknet_integration_tests::integration_test_utils::{
    create_integration_test_tx_generator,
    run_integration_test_scenario,
};

#[fixture]
fn tx_generator() -> MultiAccountTransactionGenerator {
    create_integration_test_tx_generator()
}

#[rstest]
#[tokio::test]
async fn test_end_to_end(tx_generator: MultiAccountTransactionGenerator) {
    // Setup.
    let mock_running_system = IntegrationTestSetup::new_from_tx_generator(&tx_generator).await;

    // Create and send transactions.
    let expected_batched_tx_hashes = run_integration_test_scenario(tx_generator, &|tx| {
        mock_running_system.assert_add_tx_success(tx)
    })
    .await;

    // Test.
    run_consensus_for_end_to_end_test(
        &mock_running_system.batcher_client,
        &expected_batched_tx_hashes,
    )
    .await;
}

/// This function should mirror
/// [`run_consensus`](papyrus_consensus::manager::run_consensus). It makes requests
/// from the batcher client and asserts the expected responses were received.
pub async fn run_consensus_for_end_to_end_test(
    batcher_client: &SharedBatcherClient,
    expected_batched_tx_hashes: &[TransactionHash],
) {
    // Setup. Holds the state of the consensus manager.

    // Set start height.
    // TODO(Arni): Get the current height and retrospective_block_hash from the rpc storage
    let current_height = BlockNumber(1);

    // Test.

    // Start height.
    batcher_client.start_height(StartHeightInput { height: current_height }).await.unwrap();

    // Build proposal.
    let proposal_id = ProposalId(0);
    let retrospective_block_hash = None;

    let build_proposal_duaration = chrono::TimeDelta::new(1, 0).unwrap();
    batcher_client
        .build_proposal(BuildProposalInput {
            proposal_id,
            deadline: chrono::Utc::now() + build_proposal_duaration,
            retrospective_block_hash,
        })
        .await
        .unwrap();

    // Get proposal content.
    let mut executed_tx_hashes: Vec<TransactionHash> = vec![];
    let _proposal_commitment = loop {
        let response = batcher_client
            .get_proposal_content(GetProposalContentInput { proposal_id })
            .await
            .unwrap();
        match response.content {
            GetProposalContent::Txs(batched_txs) => {
                executed_tx_hashes.append(&mut batched_txs.iter().map(|tx| tx.tx_hash()).collect());
            }
            GetProposalContent::Finished(proposal_commitment) => {
                break proposal_commitment;
            }
        }
    };

    assert_eq!(expected_batched_tx_hashes, executed_tx_hashes);
}
