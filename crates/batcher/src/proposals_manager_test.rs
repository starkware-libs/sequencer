use std::ops::Range;
use std::sync::Arc;

use assert_matches::assert_matches;
use async_trait::async_trait;
use atomic_refcell::AtomicRefCell;
use mempool_test_utils::starknet_api_test_utils::create_executable_tx;
use rstest::{fixture, rstest};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::executable_transaction::Transaction;
use starknet_api::felt;
use starknet_api::transaction::{DeprecatedResourceBoundsMapping, Tip, TransactionHash};
use starknet_mempool_types::communication::MockMempoolClient;
use tokio_stream::StreamExt;
use tracing::instrument;

use crate::proposals_manager::{
    BlockBuilderTrait,
    MockBlockBuilderFactory,
    ProposalId,
    ProposalsManager,
    ProposalsManagerConfig,
    ProposalsManagerError,
};

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
        let mock_block_builder = MockBlockBuilder::new(2);
        Box::new(mock_block_builder)
    });

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

#[rstest]
#[tokio::test]
async fn concecutive_proposal_generations_success(
    proposals_manager_config: ProposalsManagerConfig,
    mut block_builder_factory: MockBlockBuilderFactory,
    mut mempool_client: MockMempoolClient,
) {
    block_builder_factory.expect_create_block_builder().times(2).returning(|| {
        let mock_block_builder = MockBlockBuilder::new(2);
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
        .returning(|| Box::new(MockBlockBuilder::new(0)));
    let mut proposals_manager = ProposalsManager::new(
        proposals_manager_config,
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

    assert_matches!(
        another_generate_request,
        Err(ProposalsManagerError::AlreadyGeneratingProposal {
            current_generating_proposal_id,
            new_proposal_id
        }) if current_generating_proposal_id == 0 && new_proposal_id == 1
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

// Not using automock because the support for async traits is not good.
#[derive(Debug)]
struct MockBlockBuilder {
    pub n_calls_before_closing_block: usize,
    n_calls: AtomicRefCell<usize>,
}

impl MockBlockBuilder {
    fn new(n_calls_before_closing_block: usize) -> Self {
        Self { n_calls_before_closing_block, n_calls: AtomicRefCell::new(0) }
    }
}

#[async_trait]
impl BlockBuilderTrait for MockBlockBuilder {
    #[instrument(skip(txs, sender), ret)]
    async fn add_txs_and_stream(
        &self,
        txs: Vec<Transaction>,
        sender: Arc<tokio::sync::mpsc::Sender<Transaction>>,
    ) -> bool {
        unsafe {
            *self.n_calls.as_ptr() += 1;
        }
        for tx in txs {
            sender.send(tx).await.unwrap();
        }

        // Close the block after n_calls_before_closing_block calls.
        self.n_calls_before_closing_block == *self.n_calls.borrow()
    }
}

// Check that the builder was called the expected number of times.
impl Drop for MockBlockBuilder {
    fn drop(&mut self) {
        assert_eq!(self.n_calls_before_closing_block, *self.n_calls.borrow());
    }
}
