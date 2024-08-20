use std::ops::Range;
use std::sync::Arc;

use assert_matches::assert_matches;
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
    MockBlockBuilderTrait,
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

#[rstest]
#[tokio::test]
async fn proposal_generation_success(
    proposals_manager_config: ProposalsManagerConfig,
    mut block_builder_factory: MockBlockBuilderFactory,
    mut mempool_client: MockMempoolClient,
) {
    let n_txs = 2 * proposals_manager_config.max_txs_per_mempool_request;
    block_builder_factory.expect_create_block_builder().once().returning(move || {
        let mut mock_block_builder = MockBlockBuilderTrait::new();
        mock_block_builder
            .expect_start()
            .return_once(move |mempool_stream| simulate_block_builder(mempool_stream, Some(n_txs)));
        Arc::new(mock_block_builder)
    });

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

    let (handle, stream) = proposals_manager
        .generate_block_proposal(0, arbitrary_deadline(), BlockNumber(0))
        .await
        .unwrap();

    let v: Vec<_> = stream.collect().await;
    assert_eq!(v, test_txs(0..n_txs));

    // Make sure the proposal generated successfully.
    handle.await.unwrap().unwrap();
}

#[rstest]
#[tokio::test]
async fn concecutive_proposal_generations_success(
    proposals_manager_config: ProposalsManagerConfig,
    mut block_builder_factory: MockBlockBuilderFactory,
    mut mempool_client: MockMempoolClient,
) {
    let n_txs = 2 * proposals_manager_config.max_txs_per_mempool_request;
    block_builder_factory.expect_create_block_builder().times(2).returning(move || {
        let mut mock_block_builder = MockBlockBuilderTrait::new();
        mock_block_builder
            .expect_start()
            .return_once(move |mempool_stream| simulate_block_builder(mempool_stream, Some(n_txs)));
        Arc::new(mock_block_builder)
    });

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

    let (handle, stream) = proposals_manager
        .generate_block_proposal(0, arbitrary_deadline(), BlockNumber(0))
        .await
        .unwrap();

    let v: Vec<_> = stream.collect().await;
    assert_eq!(v, test_txs(0..n_txs));

    // Make sure the proposal generated successfully.
    handle.await.unwrap().unwrap();

    let (handle, stream) = proposals_manager
        .generate_block_proposal(1, arbitrary_deadline(), BlockNumber(0))
        .await
        .unwrap();

    // Make sure the proposal generated successfully.
    handle.await.unwrap().unwrap();

    let v: Vec<_> = stream.collect().await;
    assert_eq!(v, vec![]);
}

#[rstest]
#[tokio::test]
async fn multiple_proposals_generation_fail(
    proposals_manager_config: ProposalsManagerConfig,
    mut block_builder_factory: MockBlockBuilderFactory,
    mut mempool_client: MockMempoolClient,
) {
    block_builder_factory.expect_create_block_builder().once().returning(|| {
        let mut mock_block_builder = MockBlockBuilderTrait::new();
        mock_block_builder.expect_start().return_once(|mempool_stream| {
            // The block builder will never stop.
            simulate_block_builder(mempool_stream, None)
        });
        Arc::new(mock_block_builder)
    });
    mempool_client.expect_get_txs().returning(|_| Ok(vec![]));

    let mut proposals_manager = ProposalsManager::new(
        proposals_manager_config,
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
    );

    // A proposal that will never finish.
    let (_handle, _streamed_txs) = proposals_manager
        .generate_block_proposal(0, arbitrary_deadline(), BlockNumber(0))
        .await
        .unwrap();

    // Try to generate another proposal while the first one is still being generated.
    let another_generate_request =
        proposals_manager.generate_block_proposal(1, arbitrary_deadline(), BlockNumber(0)).await;
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

fn simulate_block_builder(
    mut mempool_tx_stream: InputTxStream,
    // None for taking all the transactions.
    n_txs_to_take: Option<usize>,
) -> (OutputTxStream, tokio::sync::oneshot::Receiver<bool>) {
    let (out_tx_sender, out_tx_receiver) = tokio::sync::mpsc::channel(100);
    let (done_sender, done_receiver) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let mut n_txs = 0;
        loop {
            if let Some(n_txs_to_take) = n_txs_to_take {
                if n_txs >= n_txs_to_take {
                    break;
                }
            }
            let Some(tx) = mempool_tx_stream.next().await else {
                break;
            };
            let _ = out_tx_sender.send(tx).await;
            n_txs += 1;
        }
        let _ = done_sender.send(true);
    });
    (OutputTxStream::new(out_tx_receiver), done_receiver)
}
