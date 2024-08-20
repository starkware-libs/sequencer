use std::ops::Range;
use std::sync::Arc;

use assert_matches::assert_matches;
use futures::FutureExt;
use mempool_test_utils::starknet_api_test_utils::create_executable_tx;
use rstest::{fixture, rstest};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::executable_transaction::Transaction;
use starknet_api::felt;
use starknet_api::transaction::{Tip, TransactionHash, ValidResourceBounds};
use starknet_mempool_types::communication::MockMempoolClient;
use tokio_stream::StreamExt;

use crate::proposals_manager::{
    InputTxStream,
    MockBlockBuilderFactory,
    MockBlockBuilderTraitWrapper,
    OutputTxStream,
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

#[fixture]
fn output_streaming(
    proposals_manager_config: ProposalsManagerConfig,
) -> (tokio::sync::mpsc::Sender<Transaction>, OutputTxStream) {
    let (output_content_sender, output_content_receiver) =
        tokio::sync::mpsc::channel(proposals_manager_config.outstream_content_buffer_size);
    let stream = tokio_stream::wrappers::ReceiverStream::new(output_content_receiver);
    (output_content_sender, stream)
}

#[rstest]
#[tokio::test]
async fn proposal_generation_success(
    proposals_manager_config: ProposalsManagerConfig,
    mut block_builder_factory: MockBlockBuilderFactory,
    mut mempool_client: MockMempoolClient,
    output_streaming: (tokio::sync::mpsc::Sender<Transaction>, OutputTxStream),
) {
    let n_txs = 2 * proposals_manager_config.max_txs_per_mempool_request;
    block_builder_factory.expect_create_block_builder().once().returning(
        move |mempool_tx_stream, output_content_sender| {
            let mut mock_block_builder = MockBlockBuilderTraitWrapper::new();
            mock_block_builder.expect_build_block().return_once(move |deadline| {
                simulate_block_builder(
                    deadline,
                    mempool_tx_stream,
                    output_content_sender,
                    Some(n_txs),
                )
                .boxed()
            });
            Arc::new(mock_block_builder)
        },
    );

    mempool_client.expect_get_txs().once().returning(|max_n_txs| Ok(test_txs(0..max_n_txs)));

    mempool_client
        .expect_get_txs()
        .once()
        .returning(|max_n_txs| Ok(test_txs(max_n_txs..2 * max_n_txs)));

    mempool_client.expect_get_txs().returning(|_| Ok(vec![]));

    let mut proposals_manager = ProposalsManager::new(
        proposals_manager_config.clone(),
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
    );

    let (output_content_sender, stream) = output_streaming;
    proposals_manager
        .generate_block_proposal(0, arbitrary_deadline(), BlockNumber(0), output_content_sender)
        .await
        .unwrap();

    assert_matches!(proposals_manager.await_active_proposal().await, Some(Ok(true)));
    let v: Vec<_> = stream.collect().await;
    assert_eq!(v, test_txs(0..n_txs));
}

#[rstest]
#[tokio::test]
async fn concecutive_proposal_generations_success(
    proposals_manager_config: ProposalsManagerConfig,
    mut block_builder_factory: MockBlockBuilderFactory,
    mut mempool_client: MockMempoolClient,
) {
    let n_txs = proposals_manager_config.max_txs_per_mempool_request;
    block_builder_factory.expect_create_block_builder().times(2).returning(
        move |mempool_tx_stream, output_content_sender| {
            let mut mock_block_builder = MockBlockBuilderTraitWrapper::new();
            mock_block_builder.expect_build_block().return_once(move |deadline| {
                simulate_block_builder(
                    deadline,
                    mempool_tx_stream,
                    output_content_sender,
                    Some(n_txs),
                )
                .boxed()
            });
            Arc::new(mock_block_builder)
        },
    );

    let expected_txs = test_txs(0..proposals_manager_config.max_txs_per_mempool_request);
    let mempool_txs = expected_txs.clone();
    mempool_client.expect_get_txs().returning(move |_max_n_txs| Ok(mempool_txs.clone()));

    let mut proposals_manager = ProposalsManager::new(
        proposals_manager_config.clone(),
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
    );

    let (output_content_sender, stream) = output_streaming(proposals_manager_config.clone());
    proposals_manager
        .generate_block_proposal(0, arbitrary_deadline(), BlockNumber(0), output_content_sender)
        .await
        .unwrap();

    // Make sure the first proposal generated successfully.
    assert_matches!(proposals_manager.await_active_proposal().await, Some(Ok(true)));
    let v: Vec<_> = stream.collect().await;
    assert_eq!(v, expected_txs);

    let (output_content_sender, stream) = output_streaming(proposals_manager_config);
    proposals_manager
        .generate_block_proposal(1, arbitrary_deadline(), BlockNumber(0), output_content_sender)
        .await
        .unwrap();

    // Make sure the proposal generated successfully.
    assert_matches!(proposals_manager.await_active_proposal().await, Some(Ok(true)));
    let v: Vec<_> = stream.collect().await;
    assert_eq!(v, expected_txs);
}

#[rstest]
#[tokio::test]
async fn multiple_proposals_generation_fail(
    proposals_manager_config: ProposalsManagerConfig,
    mut block_builder_factory: MockBlockBuilderFactory,
    mut mempool_client: MockMempoolClient,
) {
    block_builder_factory.expect_create_block_builder().once().returning(
        move |mempool_tx_stream, output_content_sender| {
            let mut mock_block_builder = MockBlockBuilderTraitWrapper::new();
            mock_block_builder.expect_build_block().return_once(|deadline| {
                // The block builder will never stop.
                simulate_block_builder(deadline, mempool_tx_stream, output_content_sender, None)
                    .boxed()
            });
            Arc::new(mock_block_builder)
        },
    );
    mempool_client.expect_get_txs().returning(|_| Ok(vec![]));

    let mut proposals_manager = ProposalsManager::new(
        proposals_manager_config.clone(),
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
    );

    // A proposal that will never finish.
    let (output_content_sender, _stream) = output_streaming(proposals_manager_config.clone());
    proposals_manager
        .generate_block_proposal(0, arbitrary_deadline(), BlockNumber(0), output_content_sender)
        .await
        .unwrap();

    // Try to generate another proposal while the first one is still being generated.
    let (another_output_content_sender, _another_stream) =
        output_streaming(proposals_manager_config);
    let another_generate_request = proposals_manager
        .generate_block_proposal(
            1,
            arbitrary_deadline(),
            BlockNumber(0),
            another_output_content_sender,
        )
        .await;
    assert_matches!(
        another_generate_request,
        Err(ProposalsManagerError::AlreadyGeneratingProposal {
            current_generating_proposal_id,
            new_proposal_id
        }) if current_generating_proposal_id == 0 && new_proposal_id == 1
    );
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
                ValidResourceBounds::create_for_testing(),
            )
        })
        .collect()
}

async fn simulate_block_builder(
    _deadline: tokio::time::Instant,
    mempool_tx_stream: InputTxStream,
    output_sender: tokio::sync::mpsc::Sender<Transaction>,
    n_txs_to_take: Option<usize>,
) -> bool {
    let mut mempool_tx_stream = mempool_tx_stream.take(n_txs_to_take.unwrap_or(usize::MAX));
    while let Some(tx) = mempool_tx_stream.next().await {
        output_sender.send(tx).await.unwrap();
    }
    true
}
