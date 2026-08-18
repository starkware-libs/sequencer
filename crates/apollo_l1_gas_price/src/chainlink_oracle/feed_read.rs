//! One Chainlink feed read: two view calls through the batcher, and the guards their answer passes.

// [Temporary comment] `pub` with no caller yet: the client (A9) calls `read_feed` through the
// feed accessors and narrows them to `pub(super)`.

use apollo_batcher_types::batcher_types::CallContractInput;
use apollo_batcher_types::communication::SharedBatcherClient;
use apollo_l1_gas_price_config::config::{
    ChainlinkOracleConfig,
    FreshnessWindow,
    RateBounds,
    RateBoundsConfig,
};
use apollo_l1_gas_price_types::errors::ExchangeRateOracleClientError;
use starknet_api::core::ContractAddress;
use starknet_types_core::felt::Felt;

use crate::chainlink_oracle::feed_math::{
    decode_feed_decimals,
    decode_retdata,
    rescale_to_rate_decimals,
    truncate_contract_call_error,
    ChainlinkRoundData,
    RateResult,
};
use crate::rate_bounds::check_rate_bounds;

#[cfg(test)]
#[path = "feed_read_test.rs"]
mod feed_read_test;

pub(super) const LATEST_ROUND_DATA_ENTRY_POINT: &str = "latest_round_data";
pub(super) const DECIMALS_ENTRY_POINT: &str = "decimals";

/// Everything one feed read needs: the feed's address, the bounds and pair its answer is judged
/// against, and the freshness window that answer must fall in.
#[derive(Clone, Copy, Debug)]
pub struct FeedRead {
    feed_address: ContractAddress,
    bounds: RateBounds,
    freshness: FreshnessWindow,
}

/// The reads a `ChainlinkOracleConfig` describes, judged against the bounds `bounds_config` holds.
/// One method per pair Chainlink quotes, so a read cannot be requested for the derived pair, which
/// has no feed.
pub trait ChainlinkFeeds {
    fn eth_usd_feed(&self, bounds_config: &RateBoundsConfig) -> FeedRead;
    fn strk_usd_feed(&self, bounds_config: &RateBoundsConfig) -> FeedRead;
}

impl ChainlinkFeeds for ChainlinkOracleConfig {
    fn eth_usd_feed(&self, bounds_config: &RateBoundsConfig) -> FeedRead {
        FeedRead {
            feed_address: self.eth_usd_feed_address,
            bounds: bounds_config.eth_usd_bounds(),
            freshness: self.freshness,
        }
    }

    fn strk_usd_feed(&self, bounds_config: &RateBoundsConfig) -> FeedRead {
        FeedRead {
            feed_address: self.strk_usd_feed_address,
            bounds: bounds_config.strk_usd_bounds(),
            freshness: self.freshness,
        }
    }
}

/// The feed's answer, rescaled to `RATE_DECIMALS` and checked against the feed's bounds.
pub async fn read_feed(
    batcher_client: &SharedBatcherClient,
    feed: FeedRead,
    block_timestamp: u64,
) -> RateResult {
    let pair = feed.bounds.pair;
    let pair_name = pair.pair_name();
    let feed_address = feed.feed_address;
    // The feed's `decimals` is read alongside every rate rather than cached, because a feed that
    // changes it would rescale the answer by a power of ten, and the absolute bounds are too wide
    // to catch that. STRK/USD accepts $0.0001 to $10, so an 8-decimal answer read as 6 decimals
    // passes as $3.00 instead of $0.03.
    // Sequential, not `try_join`: an error in one call would drop the other mid-flight, and the
    // local component server panics when the response channel of a dropped request closes.
    let decimals_retdata = call_view(batcher_client, feed_address, DECIMALS_ENTRY_POINT).await?;
    let round_retdata =
        call_view(batcher_client, feed_address, LATEST_ROUND_DATA_ENTRY_POINT).await?;
    let feed_decimals = decode_feed_decimals(decimals_retdata, pair)?;

    let round: ChainlinkRoundData = decode_retdata(round_retdata)?;
    if round.answer == 0 {
        return Err(ExchangeRateOracleClientError::InvalidRateError(format!(
            "{pair_name} returned a zero answer"
        )));
    }
    if block_timestamp.saturating_sub(round.updated_at) > feed.freshness.max_staleness_seconds {
        return Err(ExchangeRateOracleClientError::StaleFeedError {
            pair,
            updated_at: round.updated_at,
            block_timestamp,
            max_staleness_seconds: feed.freshness.max_staleness_seconds,
        });
    }
    // Catches a round dated ahead of the block being priced: the staleness check above saturates
    // such a subtraction to zero, which alone treats it as fresh regardless of age.
    if round.updated_at.saturating_sub(block_timestamp)
        > feed.freshness.max_future_updated_at_seconds
    {
        return Err(ExchangeRateOracleClientError::FutureFeedError {
            pair,
            updated_at: round.updated_at,
            block_timestamp,
            max_future_updated_at_seconds: feed.freshness.max_future_updated_at_seconds,
        });
    }

    let rate = rescale_to_rate_decimals(round.answer, feed_decimals)?;
    check_rate_bounds(rate, feed.bounds)?;
    Ok(rate)
}

async fn call_view(
    batcher_client: &SharedBatcherClient,
    contract_address: ContractAddress,
    entry_point: &str,
) -> Result<Vec<Felt>, ExchangeRateOracleClientError> {
    let call_result = batcher_client
        .call_contract(CallContractInput {
            contract_address,
            entry_point: entry_point.to_string(),
            calldata: vec![],
        })
        .await;
    match call_result {
        Ok(output) => Ok(output.retdata),
        Err(error) => Err(ExchangeRateOracleClientError::ContractCallError(format!(
            "{entry_point} at {contract_address}: {}",
            truncate_contract_call_error(error.to_string())
        ))),
    }
}
