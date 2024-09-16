use std::sync::Arc;

use assert_matches::assert_matches;
use starknet_mempool_types::communication::MockMempoolClient;

use crate::proposals_manager::{ProposalManager, ProposalManagerConfig, ProposalManagerError};

const GENERATION_TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(1);

#[tokio::test]
async fn multiple_proposals_generation_fails() {
    let mut mempool_client = MockMempoolClient::new();
    mempool_client.expect_get_txs().returning(|_| Ok(vec![]));
    let mut proposals_manager =
        ProposalManager::new(ProposalManagerConfig::default(), Arc::new(mempool_client));
    let (output_content_sender, _rx) = tokio::sync::mpsc::channel(1);
    proposals_manager
        .build_block_proposal(
            0,
            tokio::time::Instant::now() + GENERATION_TIMEOUT,
            output_content_sender,
        )
        .await
        .unwrap();

    let (another_output_content_sender, _another_rx) = tokio::sync::mpsc::channel(1);
    let another_generate_request = proposals_manager
        .build_block_proposal(
            1,
            tokio::time::Instant::now() + GENERATION_TIMEOUT,
            another_output_content_sender,
        )
        .await;

    assert_matches!(
        another_generate_request,
        Err(ProposalManagerError::AlreadyGeneratingProposal {
            current_generating_proposal_id,
            new_proposal_id
        }) if current_generating_proposal_id == 0 && new_proposal_id == 1
    );
}
