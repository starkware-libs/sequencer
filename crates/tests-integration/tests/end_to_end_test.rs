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
use starknet_integration_tests::integration_test_utils::create_integration_test_tx_generator;

#[fixture]
fn tx_generator() -> MultiAccountTransactionGenerator {
    create_integration_test_tx_generator()
}

#[rstest]
#[tokio::test]
async fn test_end_to_end(mut tx_generator: MultiAccountTransactionGenerator) {
    // Setup.
    let mock_running_system = IntegrationTestSetup::new_from_tx_generator(&tx_generator).await;

    let account0_invoke_nonce1 = tx_generator.account_with_id(0).generate_invoke_with_tip(1);
    let account0_invoke_nonce2 = tx_generator.account_with_id(0).generate_invoke_with_tip(2);
    let account1_invoke_nonce1 = tx_generator.account_with_id(1).generate_invoke_with_tip(3);

    let account0_invoke_nonce1_tx_hash =
        mock_running_system.assert_add_tx_success(&account0_invoke_nonce1).await;

    let account1_invoke_nonce1_tx_hash =
        mock_running_system.assert_add_tx_success(&account1_invoke_nonce1).await;

    let account0_invoke_nonce2_tx_hash =
        mock_running_system.assert_add_tx_success(&account0_invoke_nonce2).await;

    // account1_invoke_nonce1 precedes account0_invoke_nonce1 as its nonce is lower, despite the
    // higher tip of the latter. account1_invoke_nonce1 precedes account0_invoke_nonce1 as it
    // offers a higher tip, regardless of the nonce. Hence the expected tx order, regardless of
    // tx hashes, is: account1_invoke_nonce1, account0_invoke_nonce1, and account0_invoke_nonce2.
    let expected_batched_tx_hashes = [
        account1_invoke_nonce1_tx_hash,
        account0_invoke_nonce1_tx_hash,
        account0_invoke_nonce2_tx_hash,
    ];

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

    let mut actual_tx_hashes: Vec<TransactionHash> = vec![];
    let _proposal_commitment = loop {
        let response = batcher_client
            .get_proposal_content(GetProposalContentInput { proposal_id })
            .await
            .unwrap();
        match response.content {
            GetProposalContent::Txs(batched_txs) => {
                actual_tx_hashes.append(&mut batched_txs.iter().map(|tx| tx.tx_hash()).collect());
            }
            GetProposalContent::Finished(proposal_commitment) => {
                break proposal_commitment;
            }
        }
    };

    // Assert.
    assert_eq!(expected_batched_tx_hashes, actual_tx_hashes);
    // TODO(Arni): Consider assert_eq(_proposal_commitment, ..);
}
