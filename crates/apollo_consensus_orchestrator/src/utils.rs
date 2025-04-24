use std::sync::Arc;

use apollo_l1_gas_price_types::{
    EthToStrkOracleClientTrait,
    L1GasPriceProviderClient,
    PriceInfo,
    DEFAULT_ETH_TO_FRI_RATE,
};
use apollo_protobuf::consensus::ConsensusBlockInfo;
use starknet_api::block::{BlockTimestamp, GasPrice};
use tracing::warn;

use crate::metrics::CONSENSUS_L1_GAS_PRICE_PROVIDER_ERROR;

#[allow(dead_code)]
pub(crate) async fn get_oracle_rate_and_prices(
    eth_to_strk_oracle_client: Arc<dyn EthToStrkOracleClientTrait>,
    l1_gas_price_provider_client: Arc<dyn L1GasPriceProviderClient>,
    timestamp: u64,
    previous_block_info: Option<&ConsensusBlockInfo>,
    min_l1_gas_price: GasPrice,
    min_l1_data_gas_price: GasPrice,
) -> (u128, PriceInfo) {
    let (eth_to_strk_rate, price_info) = tokio::join!(
        eth_to_strk_oracle_client.eth_to_fri_rate(timestamp),
        l1_gas_price_provider_client.get_price_info(BlockTimestamp(timestamp))
    );

    if price_info.is_err() {
        CONSENSUS_L1_GAS_PRICE_PROVIDER_ERROR.increment(1);
    }

    if let (Ok(eth_to_strk_rate), Ok(price_info)) = (eth_to_strk_rate, price_info) {
        return (eth_to_strk_rate, price_info);
    }
    warn!("Failed to get oracle prices, using values from previous block info");

    if let Some(previous_block_info) = previous_block_info {
        return (
            previous_block_info.eth_to_fri_rate,
            PriceInfo {
                base_fee_per_gas: previous_block_info.l1_gas_price_wei,
                blob_fee: previous_block_info.l1_data_gas_price_wei,
            },
        );
    }
    warn!("No previous block info available, using default values");

    (
        DEFAULT_ETH_TO_FRI_RATE,
        PriceInfo { base_fee_per_gas: min_l1_gas_price, blob_fee: min_l1_data_gas_price },
    )
}

// TODO(alonl): make sure both oracles are available in docker settings and replace this function
// with the one above
pub(crate) async fn get_separate_oracle_rate_and_prices(
    eth_to_strk_oracle_client: Arc<dyn EthToStrkOracleClientTrait>,
    l1_gas_price_provider_client: Arc<dyn L1GasPriceProviderClient>,
    timestamp: u64,
    previous_block_info: Option<&ConsensusBlockInfo>,
    min_l1_gas_price: GasPrice,
    min_l1_data_gas_price: GasPrice,
) -> (u128, PriceInfo) {
    let (eth_to_strk_rate, price_info) = tokio::join!(
        eth_to_strk_oracle_client.eth_to_fri_rate(timestamp),
        l1_gas_price_provider_client.get_price_info(BlockTimestamp(timestamp))
    );

    match (eth_to_strk_rate, price_info) {
        (Ok(eth_to_strk_rate), Ok(price_info)) => (eth_to_strk_rate, price_info),
        (Ok(eth_to_strk_rate), Err(_)) => {
            warn!(
                "Failed to get oracle prices for l1_prices, using values from previous block info"
            );
            CONSENSUS_L1_GAS_PRICE_PROVIDER_ERROR.increment(1);
            let price_info = if let Some(previous_block_info) = previous_block_info {
                PriceInfo {
                    base_fee_per_gas: previous_block_info.l1_gas_price_wei,
                    blob_fee: previous_block_info.l1_data_gas_price_wei,
                }
            } else {
                warn!("No previous block info available, using minimum l1 gas prices");
                PriceInfo { base_fee_per_gas: min_l1_gas_price, blob_fee: min_l1_data_gas_price }
            };
            (eth_to_strk_rate, price_info)
        }
        (Err(_), Ok(price_info)) => {
            warn!(
                "Failed to get eth to strk conversion rate from oracle, using values from \
                 previous block info"
            );
            let eth_to_strk_rate = if let Some(previous_block_info) = previous_block_info {
                previous_block_info.eth_to_fri_rate
            } else {
                warn!(
                    "No previous block info available, using default eth to strk conversion rate"
                );
                DEFAULT_ETH_TO_FRI_RATE
            };
            (eth_to_strk_rate, price_info)
        }
        (Err(_), Err(_)) => {
            warn!(
                "Failed to get oracle prices for both eth_to_strk_rate and l1_prices, using \
                 values from previous block info"
            );
            CONSENSUS_L1_GAS_PRICE_PROVIDER_ERROR.increment(1);
            if let Some(previous_block_info) = previous_block_info {
                return (
                    previous_block_info.eth_to_fri_rate,
                    PriceInfo {
                        base_fee_per_gas: min_l1_gas_price,
                        blob_fee: min_l1_data_gas_price,
                    },
                );
            }
            warn!("No previous block info available, using default values");
            (
                DEFAULT_ETH_TO_FRI_RATE,
                PriceInfo { base_fee_per_gas: min_l1_gas_price, blob_fee: min_l1_data_gas_price },
            )
        }
    }
}
