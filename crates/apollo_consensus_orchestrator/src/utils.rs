use std::sync::Arc;

use apollo_l1_gas_price_types::{EthToStrkOracleClientTrait, L1GasPriceProviderClient, PriceInfo};
use apollo_protobuf::consensus::ConsensusBlockInfo;
use starknet_api::block::{BlockTimestamp, GasPrice};
use tracing::warn;

use crate::metrics::CONSENSUS_L1_GAS_PRICE_PROVIDER_ERROR;

pub(crate) async fn get_oracle_rate_and_prices(
    eth_to_strk_oracle_client: Arc<dyn EthToStrkOracleClientTrait>,
    l1_gas_price_provider_client: Arc<dyn L1GasPriceProviderClient>,
    timestamp: u64,
    previous_block_info: Option<&ConsensusBlockInfo>,
    default_eth_to_strk_rate: u128,
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
        default_eth_to_strk_rate,
        PriceInfo { base_fee_per_gas: min_l1_gas_price, blob_fee: min_l1_data_gas_price },
    )
}
