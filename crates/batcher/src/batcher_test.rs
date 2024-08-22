use std::sync::Arc;

use futures::StreamExt;
use starknet_api::block::BlockNumber;
use starknet_batcher_types::batcher_types::BuildProposalInput;
use starknet_mempool_types::communication::MockMempoolClient;

use crate::batcher::{Batcher, MockProposalsManagerTrait};
use crate::config::BatcherConfig;

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
        .expect_call_generate_block_proposal()
        .once()
        .withf(move |proposal_id, deadline, height| {
            *proposal_id == 0
                && instants_eq(
                    *deadline,
                    tokio::time::Instant::from_std(input_clone.deadline_as_instant().unwrap()),
                )
                && *height == BlockNumber(0)
        })
        .returning(|_, _, _| Ok(futures::stream::empty().boxed()));
    let mut batcher = Batcher::new(config, Arc::new(mempool_client), proposals_manager);

    let response = batcher.build_proposal(input).await;
    assert_eq!(response, Ok(()));
}

fn instants_eq(a: tokio::time::Instant, b: tokio::time::Instant) -> bool {
    const EPSILON: f32 = 1e-3;

    a >= b.checked_sub(tokio::time::Duration::from_secs_f32(EPSILON)).unwrap()
        && a <= b.checked_add(tokio::time::Duration::from_secs_f32(EPSILON)).unwrap()
}
