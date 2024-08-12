use std::sync::Arc;

use assert_matches::assert_matches;
use starknet_api::block::BlockNumber;
use starknet_mempool_types::communication::MockMempoolClient;

use crate::proposals_manager::{ProposalsManager, ProposalsManagerConfig, ProposalsManagerError};

const GENERATION_TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(1);

#[tokio::test]
async fn multiple_proposals_generation() {
    let mut mempool_client = MockMempoolClient::new();
    mempool_client.expect_get_txs().returning(|_| Ok(vec![]));
    let mut proposals_manager =
        ProposalsManager::new(ProposalsManagerConfig::default(), Arc::new(mempool_client));
    proposals_manager
        .generate_block_proposal(
            0,
            tokio::time::Instant::now() + GENERATION_TIMEOUT,
            BlockNumber::default(),
            None,
        )
        .await
        .unwrap();

    let another_generate_request = proposals_manager
        .generate_block_proposal(
            1,
            tokio::time::Instant::now() + GENERATION_TIMEOUT,
            BlockNumber::default(),
            None,
        )
        .await;

    match another_generate_request {
        Err(e) => assert_matches!(
            e,
            ProposalsManagerError::AlreadyGeneratingProposal {
                current_generating_proposal_id: 0,
                new_proposal_id: 1
            }
        ),
        Ok(..) => panic!("Expected AlreadyGeneratingProposal error, got Ok"),
    };
}
