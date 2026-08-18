//! The feed fixtures the Chainlink oracle tests read, and the batcher mocks that serve them.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use apollo_batcher_types::batcher_types::CallContractOutput;
use apollo_batcher_types::communication::{
    BatcherClientError,
    MockBatcherClient,
    SharedBatcherClient,
};
use apollo_batcher_types::errors::BatcherError;
use apollo_l1_gas_price_config::config::ChainlinkOracleConfig;
use apollo_l1_gas_price_types::ExchangeRate;
use starknet_api::core::ContractAddress;
use starknet_types_core::felt::Felt;

use crate::chainlink_oracle::feed_math::RATE_SCALE;
use crate::chainlink_oracle::feed_read::{DECIMALS_ENTRY_POINT, LATEST_ROUND_DATA_ENTRY_POINT};

/// The scale the Chainlink feeds report at today.
pub(super) const FEED_DECIMALS: u32 = 8;

/// The block timestamp every fixture below is dated against, and every freshness bound measured
/// against.
pub(super) const TIMESTAMP: u64 = 1_700_000_000;

/// $3000 per ETH at `FEED_DECIMALS`.
pub(super) const ETH_USD_ANSWER: u128 = 300_000_000_000;
/// $0.03 per STRK at `FEED_DECIMALS`.
pub(super) const STRK_USD_ANSWER: u128 = 3_000_000;

/// $0.03 per STRK at `RATE_DECIMALS`, which `STRK_USD_ANSWER` rescales to.
pub(super) const STRK_TO_USD_RATE: ExchangeRate = 30_000_000_000_000_000;
/// 100,000 STRK per ETH at `RATE_DECIMALS`, which the two feed answers derive to.
pub(super) const ETH_TO_FRI_RATE: ExchangeRate = 100_000 * RATE_SCALE;

/// The mocked reply per feed address and entry point. A call with no entry here fails.
pub(super) type FeedResponses = HashMap<(ContractAddress, String), Vec<Felt>>;

pub(super) fn test_config() -> ChainlinkOracleConfig {
    ChainlinkOracleConfig::default()
}

pub(super) fn fresh_updated_at() -> u64 {
    TIMESTAMP
}

pub(super) fn stale_updated_at() -> u64 {
    TIMESTAMP - test_config().freshness.max_staleness_seconds - 1
}

/// One feed's mocked `decimals` and `latest_round_data` replies, reported at `FEED_DECIMALS`.
#[derive(Clone, Copy)]
pub(super) struct FeedFixture {
    answer: u128,
    updated_at: u64,
}

impl FeedFixture {
    pub(super) fn new(answer: u128, updated_at: u64) -> Self {
        Self { answer, updated_at }
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
        ((feed_address, DECIMALS_ENTRY_POINT.to_string()), decimals_retdata(FEED_DECIMALS)),
        (
            (feed_address, LATEST_ROUND_DATA_ENTRY_POINT.to_string()),
            round_retdata(feed.answer, feed.updated_at),
        ),
    ])
}

pub(super) fn strk_usd_responses(strk_usd: FeedFixture) -> FeedResponses {
    feed_responses(test_config().strk_usd_feed_address, strk_usd)
}

pub(super) fn eth_and_strk_responses(eth_usd: FeedFixture, strk_usd: FeedFixture) -> FeedResponses {
    let mut responses = feed_responses(test_config().eth_usd_feed_address, eth_usd);
    responses.extend(strk_usd_responses(strk_usd));
    responses
}

pub(super) fn batcher_client_from_responses(responses: FeedResponses) -> SharedBatcherClient {
    counting_batcher_client(responses).0
}

/// A batcher client alongside the number of calls made through it.
pub(super) fn counting_batcher_client(
    responses: FeedResponses,
) -> (SharedBatcherClient, Arc<AtomicUsize>) {
    let num_calls = Arc::new(AtomicUsize::new(0));
    let num_calls_in_mock = num_calls.clone();
    let mut batcher_client = MockBatcherClient::new();
    batcher_client.expect_call_contract().returning(move |input| {
        num_calls_in_mock.fetch_add(1, Ordering::SeqCst);
        reply(&responses, input.contract_address, &input.entry_point)
    });
    (Arc::new(batcher_client), num_calls)
}

/// Serves `responses` for the first `num_served_calls` calls and fails every call after that, which
/// lets a test hold a successful read followed by a failing one.
pub(super) fn batcher_client_failing_after(
    responses: FeedResponses,
    num_served_calls: usize,
) -> SharedBatcherClient {
    let num_calls = AtomicUsize::new(0);
    let mut batcher_client = MockBatcherClient::new();
    batcher_client.expect_call_contract().returning(move |input| {
        if num_calls.fetch_add(1, Ordering::SeqCst) >= num_served_calls {
            return Err(BatcherClientError::BatcherError(BatcherError::InternalError));
        }
        reply(&responses, input.contract_address, &input.entry_point)
    });
    Arc::new(batcher_client)
}

fn reply(
    responses: &FeedResponses,
    contract_address: ContractAddress,
    entry_point: &str,
) -> Result<CallContractOutput, BatcherClientError> {
    responses
        .get(&(contract_address, entry_point.to_string()))
        .map(|retdata| CallContractOutput { retdata: retdata.clone() })
        .ok_or(BatcherClientError::BatcherError(BatcherError::InternalError))
}
