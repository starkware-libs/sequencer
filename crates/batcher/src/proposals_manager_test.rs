use std::ops::Range;
use std::sync::Arc;

use assert_matches::assert_matches;
use mempool_test_utils::starknet_api_test_utils::create_executable_tx;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::executable_transaction::Transaction;
use starknet_api::felt;
use starknet_api::transaction::{DeprecatedResourceBoundsMapping, Tip, TransactionHash};
use starknet_mempool_types::communication::MockMempoolClient;
use tokio_stream::StreamExt;

use crate::proposals_manager::{
    MockBlockBuilderFactory,
    MockBlockBuilderTrait,
    ProposalId,
    ProposalsManager,
    ProposalsManagerConfig,
    ProposalsManagerError,
};

#[tokio::test]
async fn proposal_generation_success() {
    let proposals_manager_config = ProposalsManagerConfig::default();
    let mut block_builder_factory = MockBlockBuilderFactory::new();
    block_builder_factory.expect_create_block_builder().once().returning(|| {
        let mut mock_block_builder = MockBlockBuilderTrait::new();
        mock_block_builder.expect_add_txs().once().returning(|_txs| false);
        // Close the block after the second call to add_txs.
        mock_block_builder.expect_add_txs().once().returning(|_txs| true);

        Box::new(mock_block_builder)
    });

    let mut mempool_client = MockMempoolClient::new();
    mempool_client.expect_get_txs().once().returning(|max_n_txs| Ok(test_txs(0..max_n_txs)));

    mempool_client
        .expect_get_txs()
        .once()
        .returning(|max_n_txs| Ok(test_txs(max_n_txs..2 * max_n_txs)));

    let mut proposals_manager = ProposalsManager::new(
        proposals_manager_config.clone(),
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
    );

    let expected_tx_hashes = (0..proposals_manager_config.max_txs_per_mempool_request * 2)
        .map(|i| TransactionHash(felt!(u8::try_from(i).unwrap())))
        .collect::<Vec<_>>();

    let streamed_txs =
        generate_block_proposal_and_collect_streamed_txs(&mut proposals_manager, 0, BlockNumber(0))
            .await;
    assert_eq!(streamed_txs, expected_tx_hashes);
}

#[tokio::test]
async fn concecutive_proposal_generations_success() {
    let proposals_manager_config = ProposalsManagerConfig::default();
    let mut block_builder_factory = MockBlockBuilderFactory::new();
    block_builder_factory.expect_create_block_builder().times(2).returning(|| {
        let mut mock_block_builder = MockBlockBuilderTrait::new();
        mock_block_builder.expect_add_txs().once().returning(|_thin_txs| false);
        // Close the block after the second call to add_txs.
        mock_block_builder.expect_add_txs().once().returning(|_thin_txs| true);

        Box::new(mock_block_builder)
    });

    let mut mempool_client = MockMempoolClient::new();
    mempool_client.expect_get_txs().once().returning(|max_n_txs| Ok(test_txs(0..max_n_txs)));
    mempool_client
        .expect_get_txs()
        .once()
        .returning(|max_n_txs| Ok(test_txs(max_n_txs..2 * max_n_txs)));
    mempool_client.expect_get_txs().once().returning(|max_n_txs| Ok(test_txs(0..max_n_txs)));
    mempool_client
        .expect_get_txs()
        .once()
        .returning(|max_n_txs| Ok(test_txs(max_n_txs..2 * max_n_txs)));

    let mut proposals_manager = ProposalsManager::new(
        proposals_manager_config.clone(),
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
    );

    let expected_tx_hashes = (0..proposals_manager_config.max_txs_per_mempool_request * 2)
        .map(|i| TransactionHash(felt!(u8::try_from(i).unwrap())))
        .collect::<Vec<_>>();

    let streamed_txs =
        generate_block_proposal_and_collect_streamed_txs(&mut proposals_manager, 0, BlockNumber(0))
            .await;
    assert_eq!(streamed_txs, expected_tx_hashes);

    let streamed_txs =
        generate_block_proposal_and_collect_streamed_txs(&mut proposals_manager, 1, BlockNumber(1))
            .await;
    assert_eq!(streamed_txs, expected_tx_hashes);
}

#[tokio::test]
async fn multiple_proposals_generation_fail() {
    let mut mempool_client = MockMempoolClient::new();
    mempool_client.expect_get_txs().returning(|_| Ok(vec![]));
    let mut block_builder_factory = MockBlockBuilderFactory::new();
    block_builder_factory
        .expect_create_block_builder()
        .once()
        .returning(|| Box::new(MockBlockBuilderTrait::new()));
    let mut proposals_manager = ProposalsManager::new(
        ProposalsManagerConfig::default(),
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
    );
    let _ = proposals_manager
        .generate_block_proposal(0, arbitrary_deadline(), BlockNumber::default())
        .await
        .unwrap();

    let another_generate_request = proposals_manager
        .generate_block_proposal(1, arbitrary_deadline(), BlockNumber::default())
        .await;

    let Err(err) = another_generate_request else {
        panic!("Expected an error, got Ok");
    };

    assert_matches!(
        err,
        ProposalsManagerError::AlreadyGeneratingProposal {
            current_generating_proposal_id,
            new_proposal_id
        } if current_generating_proposal_id == 0 && new_proposal_id == 1
    );
}

async fn generate_block_proposal_and_collect_streamed_txs(
    proposal_manager: &mut ProposalsManager,
    proposal_id: ProposalId,
    block_number: BlockNumber,
) -> Vec<TransactionHash> {
    let mut tx_stream = proposal_manager
        .generate_block_proposal(proposal_id, arbitrary_deadline(), block_number)
        .await
        .unwrap();

    let mut streamed_txs = vec![];
    while let Some(tx) = tx_stream.next().await {
        streamed_txs.push(tx.tx_hash());
    }

    streamed_txs
}

fn arbitrary_deadline() -> tokio::time::Instant {
    const GENERATION_TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(1);
    tokio::time::Instant::now() + GENERATION_TIMEOUT
}

fn test_txs(tx_hash_range: Range<usize>) -> Vec<Transaction> {
    tx_hash_range
        .map(|i| {
            create_executable_tx(
                ContractAddress::default(),
                TransactionHash(felt!(u128::try_from(i).unwrap())),
                Tip::default(),
                Nonce::default(),
                DeprecatedResourceBoundsMapping::default(),
            )
        })
        .collect()
}
