use std::sync::Arc;

use apollo_l1_gas_price_types::{
    EthToStrkOracleClientTrait,
    L1GasPriceProviderClient,
    PriceInfo,
    DEFAULT_ETH_TO_FRI_RATE,
};
use apollo_protobuf::consensus::ConsensusBlockInfo;
use num_rational::Ratio;
use starknet_api::block::{BlockTimestamp, GasPrice};
use tracing::{info, warn};

use crate::metrics::CONSENSUS_L1_GAS_PRICE_PROVIDER_ERROR;

pub(crate) struct GasPriceParams {
    pub min_l1_gas_price_wei: GasPrice,
    pub max_l1_gas_price_wei: GasPrice,
    pub max_l1_data_gas_price_wei: GasPrice,
    pub min_l1_data_gas_price_wei: GasPrice,
    pub l1_data_gas_price_multiplier: Ratio<u128>,
    pub l1_gas_tip_wei: GasPrice,
}

pub(crate) async fn get_oracle_rate_and_prices(
    eth_to_strk_oracle_client: Arc<dyn EthToStrkOracleClientTrait>,
    l1_gas_price_provider_client: Arc<dyn L1GasPriceProviderClient>,
    timestamp: u64,
    previous_block_info: Option<&ConsensusBlockInfo>,
    gas_price_params: &GasPriceParams,
) -> (u128, PriceInfo) {
    let (eth_to_strk_rate, price_info) = tokio::join!(
        eth_to_strk_oracle_client.eth_to_fri_rate(timestamp),
        l1_gas_price_provider_client.get_price_info(BlockTimestamp(timestamp))
    );

    if price_info.is_err() {
        warn!("Failed to get l1 gas price from provider: {:?}", price_info);
        CONSENSUS_L1_GAS_PRICE_PROVIDER_ERROR.increment(1);
    }
    if eth_to_strk_rate.is_err() {
        warn!("Failed to get eth to strk rate from oracle: {:?}", eth_to_strk_rate);
    }

    match (eth_to_strk_rate, price_info) {
        (Ok(eth_to_strk_rate), Ok(mut price_info)) => {
            info!("eth_to_strk_rate: {eth_to_strk_rate}, l1 gas price: {price_info:?}");
            apply_fee_transformations(&mut price_info, gas_price_params);
            return (eth_to_strk_rate, price_info);
        }
        _ => {
            warn!("Using values from previous block info.")
        }
    }

    if let Some(previous_block_info) = previous_block_info {
        let (prev_eth_to_strk_rate, prev_l1_price) = (
            previous_block_info.eth_to_fri_rate,
            PriceInfo {
                base_fee_per_gas: previous_block_info.l1_gas_price_wei,
                blob_fee: previous_block_info.l1_data_gas_price_wei,
            },
        );
        warn!(
            "previous eth_to_strk_rate: {prev_eth_to_strk_rate}, previous l1 gas price: \
             {prev_l1_price:?}"
        );
        return (prev_eth_to_strk_rate, prev_l1_price);
    }
    warn!("No previous block info available, using default values");
    warn!(
        "default eth_to_strk_rate: {DEFAULT_ETH_TO_FRI_RATE}, default (min) l1 gas price: {:?}, \
         default (min) l1 data gas price: {:?}",
        gas_price_params.min_l1_gas_price_wei, gas_price_params.min_l1_data_gas_price_wei
    );

    (
        DEFAULT_ETH_TO_FRI_RATE,
        PriceInfo {
            base_fee_per_gas: gas_price_params.min_l1_gas_price_wei,
            blob_fee: gas_price_params.min_l1_data_gas_price_wei,
        },
    )
}

fn apply_fee_transformations(price_info: &mut PriceInfo, gas_price_params: &GasPriceParams) {
    price_info.base_fee_per_gas = price_info
        .base_fee_per_gas
        .saturating_add(gas_price_params.l1_gas_tip_wei)
        .clamp(gas_price_params.min_l1_gas_price_wei, gas_price_params.max_l1_gas_price_wei);

    price_info.blob_fee = GasPrice(
        (gas_price_params.l1_data_gas_price_multiplier * price_info.blob_fee.0).to_integer(),
    )
    .clamp(gas_price_params.min_l1_data_gas_price_wei, gas_price_params.max_l1_data_gas_price_wei);
}
