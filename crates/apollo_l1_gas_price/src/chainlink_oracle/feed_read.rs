//! One Chainlink feed read: two view calls through the batcher, and the guards their answer passes.

use apollo_batcher_types::batcher_types::CallContractInput;
use apollo_batcher_types::communication::SharedBatcherClient;
use apollo_l1_gas_price_config::config::{
    AllRateBoundsConfig,
    ChainlinkOracleConfig,
    FreshnessWindow,
    RateBounds,
};
use apollo_l1_gas_price_types::errors::ExchangeRateOracleClientError;
use starknet_api::core::ContractAddress;
use starknet_types_core::felt::Felt;

use crate::chainlink_oracle::contract_call_error::truncate_contract_call_error;
use crate::chainlink_oracle::feed_decode::{decode_feed_decimals, decode_feed_round};
use crate::chainlink_oracle::feed_math::{rescale_to_rate_decimals, RateResult};
use crate::rate_bounds::check_rate_bounds;

#[cfg(test)]
#[path = "feed_read_test.rs"]
mod feed_read_test;

pub(super) const LATEST_ROUND_DATA_ENTRY_POINT: &str = "latest_round_data";
pub(super) const DECIMALS_ENTRY_POINT: &str = "decimals";

/// One quoted pair's feed: its address, the bounds and pair its answer is judged against, and the
/// freshness window that answer must fall in. The derived ETH/STRK pair has no feed and no value of
/// this type.
#[derive(Clone, Copy, Debug)]
pub(super) struct PairFeed {
    feed_address: ContractAddress,
    bounds: RateBounds,
    freshness: FreshnessWindow,
}

/// The reads a `ChainlinkOracleConfig` describes, judged against the bounds `bounds_config` holds.
/// One method per pair Chainlink quotes, so a read cannot be requested for the derived pair, which
/// has no feed.
pub(super) trait ChainlinkFeeds {
    fn eth_usd_feed(&self, bounds_config: &AllRateBoundsConfig) -> PairFeed;
    fn strk_usd_feed(&self, bounds_config: &AllRateBoundsConfig) -> PairFeed;
}

impl ChainlinkFeeds for ChainlinkOracleConfig {
    fn eth_usd_feed(&self, bounds_config: &AllRateBoundsConfig) -> PairFeed {
        PairFeed {
            feed_address: self.eth_usd_feed_address,
            bounds: bounds_config.eth_usd_bounds(),
            freshness: self.freshness,
        }
    }

    fn strk_usd_feed(&self, bounds_config: &AllRateBoundsConfig) -> PairFeed {
        PairFeed {
            feed_address: self.strk_usd_feed_address,
            bounds: bounds_config.strk_usd_bounds(),
            freshness: self.freshness,
        }
    }
}

/// The feed's answer, rescaled to `EXCHANGE_RATE_DECIMALS` and checked against the feed's bounds.
pub(super) async fn read_feed(
    batcher_client: &SharedBatcherClient,
    feed: PairFeed,
    block_timestamp: u64,
) -> RateResult {
    let pair = feed.bounds.pair;
    let pair_name = pair.pair_name();
    let feed_address = feed.feed_address;
    // `decimals` is read with every rate rather than cached: a feed that changes it rescales the
    // answer by a power of ten, which the absolute bounds are too wide to catch.
    let decimals_retdata = call_view(batcher_client, feed_address, DECIMALS_ENTRY_POINT).await?;
    let round_retdata =
        call_view(batcher_client, feed_address, LATEST_ROUND_DATA_ENTRY_POINT).await?;
    let feed_decimals = decode_feed_decimals(decimals_retdata, pair)?;

    let round = decode_feed_round(round_retdata)?;
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
