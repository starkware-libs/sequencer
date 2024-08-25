use std::sync::Arc;

use futures::StreamExt;
use starknet_api::block::BlockNumber;
use starknet_batcher_types::batcher_types::{
    BuildProposalInput,
    GetStreamContentInput,
    ProposalContentId,
    StreamContent,
};
use starknet_batcher_types::errors::BatcherError;
use starknet_mempool_types::communication::MockMempoolClient;

use crate::batcher::Batcher;
use crate::config::BatcherConfig;
use crate::proposals_manager::MockProposalsManagerTrait;
use crate::test_utils::test_txs;

// TODO: Consider deleting this test and leaving only build_proposal_stream_txs as this flow is the
// setup of the other test.
#[tokio::test]
async fn build_proposal_success() {
    let input = BuildProposalInput {
        deadline: chrono::Utc::now() + chrono::Duration::seconds(1),
        height: BlockNumber(0),
        stream_id: 0,
    };
    let input_clone = input.clone();

    let config = BatcherConfig::default();
    let mempool_client = MockMempoolClient::new();
    let mut proposals_manager = MockProposalsManagerTrait::new();
    proposals_manager
        .expect_generate_block_proposal()
        .once()
        .withf(move |deadline, height| {
            instants_eq(
                *deadline,
                tokio::time::Instant::from_std(input_clone.deadline_as_instant().unwrap()),
            ) && *height == BlockNumber(0)
        })
        .returning(|_, _| Ok(futures::stream::empty().boxed()));
    let mut batcher = Batcher::new(config, Arc::new(mempool_client), proposals_manager);

    let response = batcher.build_proposal(&input).await;
    assert_eq!(response, Ok(()));
}

#[tokio::test]
async fn get_stream_content_success() {
    let build_proposal_input = BuildProposalInput {
        deadline: chrono::Utc::now() + chrono::Duration::seconds(1),
        height: BlockNumber(0),
        stream_id: 0,
    };
    let build_proposal_input_clone = build_proposal_input.clone();
    let expected_txs = test_txs(0..10);
    let expected_txs_clone = expected_txs.clone();

    let config = BatcherConfig::default();
    let mempool_client = MockMempoolClient::new();
    let mut proposals_manager = MockProposalsManagerTrait::new();
    proposals_manager
        .expect_generate_block_proposal()
        .once()
        .withf(move |deadline, height| {
            instants_eq(
                *deadline,
                tokio::time::Instant::from_std(
                    build_proposal_input_clone.deadline_as_instant().unwrap(),
                ),
            ) && *height == BlockNumber(0)
        })
        .returning(move |_, _| Ok(futures::stream::iter(expected_txs.clone()).boxed()));
    let mut batcher = Batcher::new(config, Arc::new(mempool_client), proposals_manager);

    let response = batcher.build_proposal(&build_proposal_input).await;
    assert_eq!(response, Ok(()));

    let get_stream_content_input = GetStreamContentInput { stream_id: 0 };
    for expected_tx in expected_txs_clone {
        let next_tx = batcher.get_stream_content(&get_stream_content_input).await.unwrap();
        assert_eq!(next_tx, StreamContent::Tx(expected_tx));
    }
    let closing_value = batcher.get_stream_content(&get_stream_content_input).await.unwrap();
    assert_eq!(closing_value, StreamContent::StreamEnd(ProposalContentId::default()));
    let stream_deleted = batcher.get_stream_content(&get_stream_content_input).await;
    assert_eq!(stream_deleted, Err(BatcherError::StreamIdDoesNotExist { stream_id: 0 }));
}

#[tokio::test]
async fn get_stream_content_illegal_input_fails() {
    let config = BatcherConfig::default();
    let mempool_client = MockMempoolClient::new();
    let proposals_manager = MockProposalsManagerTrait::new();
    let mut batcher = Batcher::new(config, Arc::new(mempool_client), proposals_manager);

    let get_stream_content_input = GetStreamContentInput { stream_id: 0 };
    let err = batcher.get_stream_content(&get_stream_content_input).await.unwrap_err();
    assert_eq!(err, BatcherError::StreamIdDoesNotExist { stream_id: 0 });
}

fn instants_eq(a: tokio::time::Instant, b: tokio::time::Instant) -> bool {
    const EPSILON: f32 = 1e-3;

    a >= b.checked_sub(tokio::time::Duration::from_secs_f32(EPSILON)).unwrap()
        && a <= b.checked_add(tokio::time::Duration::from_secs_f32(EPSILON)).unwrap()
}
