use std::sync::Arc;

use assert_matches::assert_matches;
use async_trait::async_trait;
use atomic_refcell::AtomicRefCell;
use rstest::{fixture, rstest};
use starknet_api::block::BlockNumber;
use starknet_api::core::TransactionCommitment;
use starknet_api::executable_transaction::Transaction;
use starknet_api::felt;
use starknet_api::transaction::TransactionHash;
use starknet_batcher_types::batcher_types::ProposalContentId;
use starknet_mempool_types::communication::MockMempoolClient;
use starknet_types_core::felt::Felt;
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use tracing::instrument;

use crate::proposals_manager::{
    BlockBuilderTrait,
    ClosedBlock,
    MockBlockBuilderFactory,
    MockStorageWriterTrait,
    ProposalsManager,
    ProposalsManagerConfig,
    ProposalsManagerError,
    ProposalsManagerTrait,
};
use crate::test_utils::test_txs;

#[fixture]
fn proposals_manager_config() -> ProposalsManagerConfig {
    ProposalsManagerConfig::default()
}

// TODO: Figure out how to pass expectations to the mock.
#[fixture]
fn block_builder_factory() -> MockBlockBuilderFactory {
    MockBlockBuilderFactory::new()
}

// TODO: Figure out how to pass expectations to the mock.
#[fixture]
fn mempool_client() -> MockMempoolClient {
    MockMempoolClient::new()
}

#[rstest]
#[tokio::test]
async fn proposal_generation_success(
    proposals_manager_config: ProposalsManagerConfig,
    mut block_builder_factory: MockBlockBuilderFactory,
    mut mempool_client: MockMempoolClient,
) {
    block_builder_factory.expect_create_block_builder().once().returning(|| {
        let mock_block_builder = MockBlockBuilder::new(2, ClosedBlock::default());
        Box::new(mock_block_builder)
    });

    mempool_client.expect_get_txs().once().returning(|max_n_txs| Ok(test_txs(0..max_n_txs)));

    mempool_client
        .expect_get_txs()
        .once()
        .returning(|max_n_txs| Ok(test_txs(max_n_txs..2 * max_n_txs)));

    let storage_writer = MockStorageWriterTrait::new();

    let mut proposals_manager = ProposalsManager::new(
        proposals_manager_config.clone(),
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
        Arc::new(Mutex::new(storage_writer)),
    );

    let expected_tx_hashes = (0..proposals_manager_config.max_txs_per_mempool_request * 2)
        .map(|i| TransactionHash(felt!(u8::try_from(i).unwrap())))
        .collect::<Vec<_>>();

    let streamed_txs =
        generate_block_proposal_and_collect_streamed_txs(&mut proposals_manager, BlockNumber(0))
            .await;
    assert_eq!(streamed_txs, expected_tx_hashes);
}

#[rstest]
#[tokio::test]
async fn concecutive_proposal_generations_success(
    proposals_manager_config: ProposalsManagerConfig,
    mut block_builder_factory: MockBlockBuilderFactory,
    mut mempool_client: MockMempoolClient,
) {
    block_builder_factory.expect_create_block_builder().times(2).returning(|| {
        let mock_block_builder = MockBlockBuilder::new(2, ClosedBlock::default());
        Box::new(mock_block_builder)
    });

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

    let storage_writer = MockStorageWriterTrait::new();

    let mut proposals_manager = ProposalsManager::new(
        proposals_manager_config.clone(),
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
        Arc::new(Mutex::new(storage_writer)),
    );

    let expected_tx_hashes = (0..proposals_manager_config.max_txs_per_mempool_request * 2)
        .map(|i| TransactionHash(felt!(u8::try_from(i).unwrap())))
        .collect::<Vec<_>>();

    let streamed_txs =
        generate_block_proposal_and_collect_streamed_txs(&mut proposals_manager, BlockNumber(0))
            .await;
    assert_eq!(streamed_txs, expected_tx_hashes);

    let streamed_txs =
        generate_block_proposal_and_collect_streamed_txs(&mut proposals_manager, BlockNumber(1))
            .await;
    assert_eq!(streamed_txs, expected_tx_hashes);
}

#[rstest]
#[tokio::test]
async fn multiple_proposals_generation_fail(
    proposals_manager_config: ProposalsManagerConfig,
    mut block_builder_factory: MockBlockBuilderFactory,
    mut mempool_client: MockMempoolClient,
) {
    mempool_client.expect_get_txs().returning(|_| Ok(vec![]));
    block_builder_factory
        .expect_create_block_builder()
        .once()
        .returning(|| Box::new(MockBlockBuilder::new(0, ClosedBlock::default())));
    let storage_writer = MockStorageWriterTrait::new();
    let mut proposals_manager = ProposalsManager::new(
        proposals_manager_config,
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
        Arc::new(Mutex::new(storage_writer)),
    );
    let _ = proposals_manager
        .generate_block_proposal(arbitrary_deadline(), BlockNumber::default())
        .await
        .unwrap();

    let another_generate_request = proposals_manager
        .generate_block_proposal(arbitrary_deadline(), BlockNumber::default())
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

#[rstest]
#[tokio::test]
async fn decision_reached_without_proposals_fail(
    proposals_manager_config: ProposalsManagerConfig,
    mempool_client: MockMempoolClient,
    block_builder_factory: MockBlockBuilderFactory,
) {
    let storage_writer = MockStorageWriterTrait::new();

    let mut proposals_manager = ProposalsManager::new(
        proposals_manager_config,
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
        Arc::new(Mutex::new(storage_writer)),
    );

    assert_matches!(
        proposals_manager.decision_reached(BlockNumber(0), ProposalContentId::default()).await,
        Err(ProposalsManagerError::ClosedBlockNotFound {
            height,
            content_id,
        }) if height == BlockNumber(0) && content_id == ProposalContentId::default()
    );
}

#[rstest]
#[tokio::test]
async fn decision_reached_proposal_exists_success(
    proposals_manager_config: ProposalsManagerConfig,
    mut block_builder_factory: MockBlockBuilderFactory,
    mut mempool_client: MockMempoolClient,
) {
    // Create a block builder that will return a closed block with content id 1.
    block_builder_factory.expect_create_block_builder().once().returning(|| {
        Box::new(MockBlockBuilder::new(
            1,
            ClosedBlock {
                content_id: ProposalContentId { tx_commitment: TransactionCommitment(Felt::ONE) },
                height: BlockNumber(0),
                ..Default::default()
            },
        ))
    });
    // Create another block builder that will return a closed block with content id 2.
    block_builder_factory.expect_create_block_builder().once().returning(|| {
        Box::new(MockBlockBuilder::new(
            1,
            ClosedBlock {
                content_id: ProposalContentId { tx_commitment: TransactionCommitment(Felt::ONE) },
                height: BlockNumber(0),
                ..Default::default()
            },
        ))
    });

    // Txs for the first proposal.
    mempool_client.expect_get_txs().once().returning(|max_n_txs| Ok(test_txs(0..max_n_txs)));
    // Txs for the second proposal.
    mempool_client.expect_get_txs().once().returning(|max_n_txs| Ok(test_txs(0..max_n_txs)));

    let mut storage_writer = MockStorageWriterTrait::new();
    // Expect the first proposal to be chosen.
    storage_writer
        .expect_commit_block()
        .once()
        .withf(|closed_block| {
            closed_block.content_id
                == ProposalContentId { tx_commitment: TransactionCommitment(Felt::ONE) }
        })
        .returning(|_closed_block| Ok(()));

    let mut proposals_manager = ProposalsManager::new(
        proposals_manager_config.clone(),
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
        Arc::new(Mutex::new(storage_writer)),
    );

    let _stream0 = proposals_manager
        .generate_block_proposal(arbitrary_deadline(), BlockNumber(0))
        .await
        .unwrap();

    // Make sure the first proposal finished building before proposing the second one.
    tokio::time::sleep(arbitrary_deadline().duration_since(tokio::time::Instant::now())).await;

    let _stream1 = proposals_manager
        .generate_block_proposal(arbitrary_deadline(), BlockNumber(0))
        .await
        .unwrap();

    // Make sure the second proposal finished building before making a decision.
    tokio::time::sleep(arbitrary_deadline().duration_since(tokio::time::Instant::now())).await;

    proposals_manager
        .decision_reached(
            BlockNumber(0),
            ProposalContentId { tx_commitment: TransactionCommitment(Felt::ONE) },
        )
        .await
        .unwrap();
}

async fn generate_block_proposal_and_collect_streamed_txs(
    proposal_manager: &mut ProposalsManager,
    block_number: BlockNumber,
) -> Vec<TransactionHash> {
    let mut tx_stream =
        proposal_manager.generate_block_proposal(arbitrary_deadline(), block_number).await.unwrap();

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
// Not using automock because the support for async traits is not good.
#[derive(Debug)]
struct MockBlockBuilder {
    pub n_calls_before_closing_block: usize,
    pub closed_block: ClosedBlock,
    n_calls: AtomicRefCell<usize>,
}

impl MockBlockBuilder {
    fn new(n_calls_before_closing_block: usize, closed_block: ClosedBlock) -> Self {
        Self { n_calls_before_closing_block, closed_block, n_calls: AtomicRefCell::new(0) }
    }
}

#[async_trait]
impl BlockBuilderTrait for MockBlockBuilder {
    #[instrument(skip(txs, sender), ret)]
    async fn add_txs_and_stream(
        &self,
        txs: Vec<Transaction>,
        sender: Arc<tokio::sync::mpsc::Sender<Transaction>>,
    ) -> Option<ClosedBlock> {
        unsafe {
            *self.n_calls.as_ptr() += 1;
        }
        for tx in txs {
            sender.send(tx).await.unwrap();
        }

        // Close the block after n_calls_before_closing_block calls.
        if self.n_calls_before_closing_block == *self.n_calls.borrow() {
            Some(self.closed_block.clone())
        } else {
            None
        }
    }
}

// Check that the builder was called the expected number of times.
impl Drop for MockBlockBuilder {
    fn drop(&mut self) {
        assert_eq!(self.n_calls_before_closing_block, *self.n_calls.borrow());
    }
}
