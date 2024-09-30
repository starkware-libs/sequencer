use std::ops::Range;
use std::sync::Arc;

use assert_matches::assert_matches;
use async_trait::async_trait;
use futures::future::BoxFuture;
use futures::FutureExt;
#[cfg(test)]
use mockall::automock;
use rstest::{fixture, rstest};
use starknet_api::executable_transaction::Transaction;
use starknet_api::felt;
use starknet_api::test_utils::invoke::{executable_invoke_tx, InvokeTxArgs};
use starknet_api::transaction::TransactionHash;
use starknet_batcher_types::batcher_types::ProposalId;
use starknet_mempool_types::communication::MockMempoolClient;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use crate::proposal_manager::{
    BlockBuilderTrait,
    InputTxStream,
    MockBlockBuilderFactoryTrait,
    ProposalManager,
    ProposalManagerConfig,
    ProposalManagerError,
};

pub type OutputTxStream = ReceiverStream<Transaction>;

#[fixture]
fn proposal_manager_config() -> ProposalManagerConfig {
    ProposalManagerConfig::default()
}

// TODO: Figure out how to pass expectations to the mock.
#[fixture]
fn block_builder_factory() -> MockBlockBuilderFactoryTrait {
    MockBlockBuilderFactoryTrait::new()
}

// TODO: Figure out how to pass expectations to the mock.
#[fixture]
fn mempool_client() -> MockMempoolClient {
    MockMempoolClient::new()
}

#[fixture]
fn output_streaming() -> (tokio::sync::mpsc::Sender<Transaction>, OutputTxStream) {
    const OUTPUT_CONTENT_BUFFER_SIZE: usize = 100;
    let (output_content_sender, output_content_receiver) =
        tokio::sync::mpsc::channel(OUTPUT_CONTENT_BUFFER_SIZE);
    let stream = tokio_stream::wrappers::ReceiverStream::new(output_content_receiver);
    (output_content_sender, stream)
}

#[rstest]
#[tokio::test]
async fn proposal_generation_success(
    proposal_manager_config: ProposalManagerConfig,
    mut block_builder_factory: MockBlockBuilderFactoryTrait,
    mut mempool_client: MockMempoolClient,
    output_streaming: (tokio::sync::mpsc::Sender<Transaction>, OutputTxStream),
) {
    let n_txs = 2 * proposal_manager_config.max_txs_per_mempool_request;
    block_builder_factory
        .expect_create_block_builder()
        .once()
        .returning(move || simulate_build_block(Some(n_txs)));

    mempool_client.expect_get_txs().once().returning(|max_n_txs| Ok(test_txs(0..max_n_txs)));

    mempool_client
        .expect_get_txs()
        .once()
        .returning(|max_n_txs| Ok(test_txs(max_n_txs..2 * max_n_txs)));

    mempool_client.expect_get_txs().returning(|_| Ok(vec![]));

    let mut proposal_manager = ProposalManager::new(
        proposal_manager_config.clone(),
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
    );

    let (output_content_sender, stream) = output_streaming;
    proposal_manager
        .build_block_proposal(ProposalId(0), arbitrary_deadline(), output_content_sender)
        .await
        .unwrap();

    assert_matches!(proposal_manager.await_active_proposal().await, Some(Ok(())));
    let proposal_content: Vec<_> = stream.collect().await;
    assert_eq!(proposal_content, test_txs(0..n_txs));
}

#[rstest]
#[tokio::test]
async fn consecutive_proposal_generations_success(
    proposal_manager_config: ProposalManagerConfig,
    mut block_builder_factory: MockBlockBuilderFactoryTrait,
    mut mempool_client: MockMempoolClient,
) {
    let n_txs = proposal_manager_config.max_txs_per_mempool_request;
    block_builder_factory
        .expect_create_block_builder()
        .times(2)
        .returning(move || simulate_build_block(Some(n_txs)));

    let expected_txs = test_txs(0..proposal_manager_config.max_txs_per_mempool_request);
    let mempool_txs = expected_txs.clone();
    mempool_client.expect_get_txs().returning(move |_max_n_txs| Ok(mempool_txs.clone()));

    let mut proposal_manager = ProposalManager::new(
        proposal_manager_config.clone(),
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
    );

    let (output_content_sender, stream) = output_streaming();
    proposal_manager
        .build_block_proposal(ProposalId(0), arbitrary_deadline(), output_content_sender)
        .await
        .unwrap();

    // Make sure the first proposal generated successfully.
    assert_matches!(proposal_manager.await_active_proposal().await, Some(Ok(())));
    let v: Vec<_> = stream.collect().await;
    assert_eq!(v, expected_txs);

    let (output_content_sender, stream) = output_streaming();
    proposal_manager
        .build_block_proposal(ProposalId(1), arbitrary_deadline(), output_content_sender)
        .await
        .unwrap();

    // Make sure the proposal generated successfully.
    assert_matches!(proposal_manager.await_active_proposal().await, Some(Ok(())));
    let proposal_content: Vec<_> = stream.collect().await;
    assert_eq!(proposal_content, expected_txs);
}

#[rstest]
#[tokio::test]
async fn multiple_proposals_generation_fail(
    proposal_manager_config: ProposalManagerConfig,
    mut block_builder_factory: MockBlockBuilderFactoryTrait,
    mut mempool_client: MockMempoolClient,
) {
    // The block builder will never stop.
    block_builder_factory
        .expect_create_block_builder()
        .once()
        .returning(|| simulate_build_block(None));

    mempool_client.expect_get_txs().returning(|_| Ok(vec![]));

    let mut proposal_manager = ProposalManager::new(
        proposal_manager_config.clone(),
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
    );

    // A proposal that will never finish.
    let (output_content_sender, _stream) = output_streaming();
    proposal_manager
        .build_block_proposal(ProposalId(0), arbitrary_deadline(), output_content_sender)
        .await
        .unwrap();

    // Try to generate another proposal while the first one is still being generated.
    let (another_output_content_sender, _another_stream) = output_streaming();
    let another_generate_request = proposal_manager
        .build_block_proposal(ProposalId(1), arbitrary_deadline(), another_output_content_sender)
        .await;
    assert_matches!(
        another_generate_request,
        Err(ProposalManagerError::AlreadyGeneratingProposal {
            current_generating_proposal_id,
            new_proposal_id
        }) if current_generating_proposal_id == ProposalId(0) && new_proposal_id == ProposalId(1)
    );
}

fn arbitrary_deadline() -> tokio::time::Instant {
    const GENERATION_TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(1);
    tokio::time::Instant::now() + GENERATION_TIMEOUT
}

fn test_txs(tx_hash_range: Range<usize>) -> Vec<Transaction> {
    tx_hash_range
        .map(|i| {
            Transaction::Invoke(executable_invoke_tx(InvokeTxArgs {
                tx_hash: TransactionHash(felt!(u128::try_from(i).unwrap())),
                ..Default::default()
            }))
        })
        .collect()
}

fn simulate_build_block(n_txs: Option<usize>) -> Arc<dyn BlockBuilderTrait> {
    let mut mock_block_builder = MockBlockBuilderTraitWrapper::new();
    mock_block_builder.expect_build_block().return_once(
        move |deadline, mempool_tx_stream, output_content_sender| {
            simulate_block_builder(deadline, mempool_tx_stream, output_content_sender, n_txs)
                .boxed()
        },
    );
    Arc::new(mock_block_builder)
}

async fn simulate_block_builder(
    _deadline: tokio::time::Instant,
    mempool_tx_stream: InputTxStream,
    output_sender: tokio::sync::mpsc::Sender<Transaction>,
    n_txs_to_take: Option<usize>,
) {
    let mut mempool_tx_stream = mempool_tx_stream.take(n_txs_to_take.unwrap_or(usize::MAX));
    while let Some(tx) = mempool_tx_stream.next().await {
        output_sender.send(tx).await.unwrap();
    }
}

// A wrapper trait to allow mocking the BlockBuilderTrait in tests.
#[cfg_attr(test, automock)]
trait BlockBuilderTraitWrapper: Send + Sync {
    // Equivalent to: async fn build_block(&self, deadline: tokio::time::Instant);
    fn build_block(
        &self,
        deadline: tokio::time::Instant,
        tx_stream: InputTxStream,
        output_content_sender: tokio::sync::mpsc::Sender<Transaction>,
    ) -> BoxFuture<'_, ()>;
}

#[async_trait]
impl<T: BlockBuilderTraitWrapper> BlockBuilderTrait for T {
    async fn build_block(
        &self,
        deadline: tokio::time::Instant,
        tx_stream: InputTxStream,
        output_content_sender: tokio::sync::mpsc::Sender<Transaction>,
    ) {
        self.build_block(deadline, tx_stream, output_content_sender).await
    }
}
