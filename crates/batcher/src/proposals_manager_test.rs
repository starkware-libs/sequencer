use std::sync::Arc;

use assert_matches::assert_matches;
use starknet_api::block::BlockNumber;
use starknet_api::state::StateDiff;
use starknet_mempool_types::communication::MockMempoolClient;
use starknet_mempool_types::mempool_types::ThinTransaction;

use crate::proposals_manager::{
    MockBlockBuilderFactory,
    MockBlockBuilderTrait,
    ProposalsManager,
    ProposalsManagerConfig,
    ProposalsManagerError,
};

#[tokio::test]
async fn proposal_generation_succeeds() {
    let mut block_builder_factory = MockBlockBuilderFactory::new();
    block_builder_factory.expect_create_block_builder().once().returning(|| {
        let mut mock_block_builder = MockBlockBuilderTrait::new();
        // We expect to add transactions to the arbitrary amount of times.
        mock_block_builder.expect_add_txs().times(2).returning(|_thin_txs| false);
        // Now let's return true to finish the block.
        mock_block_builder.expect_add_txs().once().returning(|_thin_txs| true);
        mock_block_builder.expect_get_state_diff().once().returning(StateDiff::default);

        Box::new(mock_block_builder)
    });

    // We expect to fetch transactions 3 times: 2 times when the block is not ready and another time
    // that will close the block.
    let mut mempool_client = MockMempoolClient::new();
    mempool_client
        .expect_get_txs()
        .times(3)
        .returning(|_| Ok(vec![ThinTransaction::default(); 10]));

    let mut proposals_manager = ProposalsManager::new(
        ProposalsManagerConfig::default(),
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
    );
    proposals_manager.generate_block_proposal(0, BlockNumber::default(), None).await.unwrap();

    let (proposal_id, state_diff) = proposals_manager.wait_for_ready_block().await.unwrap();

    assert_eq!(proposal_id, 0);
    assert_eq!(state_diff, Default::default());
}

#[tokio::test]
async fn consecutive_proposals_generation_succeed() {
    let mut block_builder_factory = MockBlockBuilderFactory::new();
    block_builder_factory.expect_create_block_builder().times(2).returning(|| {
        let mut mock_block_builder = MockBlockBuilderTrait::new();
        // We expect to add transactions to the arbitrary amount of times.
        mock_block_builder.expect_add_txs().times(2).returning(|_thin_txs| false);
        // Now let's return true to finish the block.
        mock_block_builder.expect_add_txs().once().returning(|_thin_txs| true);
        mock_block_builder.expect_get_state_diff().once().returning(StateDiff::default);

        Box::new(mock_block_builder)
    });

    // We expect to fetch transactions 6 times: 3 fetches per block. 2 times when the block is not
    // ready and another time that will close the block.
    let mut mempool_client = MockMempoolClient::new();
    mempool_client
        .expect_get_txs()
        .times(6)
        .returning(|_| Ok(vec![ThinTransaction::default(); 10]));

    let mut proposals_manager = ProposalsManager::new(
        ProposalsManagerConfig::default(),
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
    );
    proposals_manager.generate_block_proposal(0, BlockNumber::default(), None).await.unwrap();

    let (proposal_id, state_diff) = proposals_manager.wait_for_ready_block().await.unwrap();

    assert_eq!(proposal_id, 0);
    assert_eq!(state_diff, Default::default());

    proposals_manager.generate_block_proposal(1, BlockNumber(1), None).await.unwrap();

    let (proposal_id, state_diff) = proposals_manager.wait_for_ready_block().await.unwrap();

    assert_eq!(proposal_id, 1);
    assert_eq!(state_diff, Default::default());
}


#[tokio::test]
async fn multiple_proposals_generation_fails() {
    let block_builder_factory = MockBlockBuilderFactory::new();
    let mut mempool_client = MockMempoolClient::new();
    mempool_client.expect_get_txs().returning(|_| Ok(vec![]));
    let mut proposals_manager = ProposalsManager::new(
        ProposalsManagerConfig::default(),
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
    );
    proposals_manager.generate_block_proposal(0, BlockNumber::default(), None).await.unwrap();

    let another_generate_request =
        proposals_manager.generate_block_proposal(1, BlockNumber::default(), None).await;
    assert_matches!(
        another_generate_request.unwrap_err(),
        ProposalsManagerError::AlreadyGeneratingProposal {
            current_generating_proposal_id: 0,
            new_proposal_id: 1
        }
    );
}
