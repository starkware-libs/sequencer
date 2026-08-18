//! The batcher mock the Chainlink oracle tests read their feeds through.

use std::collections::HashMap;
use std::sync::Arc;

use apollo_batcher_types::batcher_types::CallContractOutput;
use apollo_batcher_types::communication::{
    BatcherClientError,
    MockBatcherClient,
    SharedBatcherClient,
};
use apollo_batcher_types::errors::BatcherError;
use starknet_api::core::ContractAddress;
use starknet_types_core::felt::Felt;

use crate::chainlink_oracle::feed_read::{DECIMALS_ENTRY_POINT, LATEST_ROUND_DATA_ENTRY_POINT};

/// The scale the Chainlink feeds report at today.
pub(super) const FEED_DECIMALS: u32 = 8;

/// The mocked reply per feed address and entry point. A call with no entry here fails.
pub(super) type FeedResponses = HashMap<(ContractAddress, String), Vec<Felt>>;

/// One feed's mocked `decimals` and `latest_round_data` replies.
#[derive(Clone, Copy)]
pub(super) struct FeedFixture {
    answer: u128,
    updated_at: u64,
    feed_decimals: u32,
}

impl FeedFixture {
    pub(super) fn new(answer: u128, updated_at: u64) -> Self {
        Self { answer, updated_at, feed_decimals: FEED_DECIMALS }
    }
}

pub(super) fn decimals_retdata(feed_decimals: u32) -> Vec<Felt> {
    vec![Felt::from(feed_decimals)]
}

pub(super) fn round_retdata(answer: u128, updated_at: u64) -> Vec<Felt> {
    // A realistic phase-encoded `round_id`: `(phase_id << 128) | aggregator_round_id`, which
    // exceeds u64.
    const PHASE_ENCODED_ROUND_ID: &str = "0x100000000000000000000000000000042";
    const BLOCK_NUMBER: u64 = 987_654;
    const STARTED_AT: u64 = 1_699_999_000;
    vec![
        Felt::from_hex_unchecked(PHASE_ENCODED_ROUND_ID),
        Felt::from(answer),
        Felt::from(BLOCK_NUMBER),
        Felt::from(STARTED_AT),
        Felt::from(updated_at),
    ]
}

pub(super) fn feed_responses(feed_address: ContractAddress, feed: FeedFixture) -> FeedResponses {
    HashMap::from([
        ((feed_address, DECIMALS_ENTRY_POINT.to_string()), decimals_retdata(feed.feed_decimals)),
        (
            (feed_address, LATEST_ROUND_DATA_ENTRY_POINT.to_string()),
            round_retdata(feed.answer, feed.updated_at),
        ),
    ])
}

pub(super) fn batcher_client_from_responses(responses: FeedResponses) -> SharedBatcherClient {
    let mut batcher_client = MockBatcherClient::new();
    batcher_client.expect_call_contract().returning(move |input| {
        responses
            .get(&(input.contract_address, input.entry_point.clone()))
            .map(|retdata| CallContractOutput { retdata: retdata.clone() })
            .ok_or(BatcherClientError::BatcherError(BatcherError::InternalError))
    });
    Arc::new(batcher_client)
}
