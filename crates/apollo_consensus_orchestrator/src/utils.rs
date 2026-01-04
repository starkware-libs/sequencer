use std::cmp::min;
use std::sync::Arc;
use std::time::Duration;

use apollo_consensus_orchestrator_config::config::ContextConfig;
use apollo_l1_gas_price_types::{L1GasPriceProviderClient, PriceInfo, DEFAULT_ETH_TO_FRI_RATE};
use apollo_protobuf::consensus::{ConsensusBlockInfo, ProposalPart};
use apollo_state_sync_types::communication::{
    StateSyncClient,
    StateSyncClientError,
    StateSyncClientResult,
};
use apollo_state_sync_types::errors::StateSyncError;
use apollo_time::time::{Clock, DateTime};
// TODO(Gilad): Define in consensus, either pass to blockifier as config or keep the dup.
use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
use futures::channel::mpsc;
use futures::SinkExt;
use num_rational::Ratio;
use starknet_api::block::{
    BlockHashAndNumber,
    BlockNumber,
    BlockTimestamp,
    GasPrice,
    GasPriceVector,
    GasPrices,
    NonzeroGasPrice,
    WEI_PER_ETH,
};
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::StarknetApiError;
use tracing::{info, warn};

use crate::build_proposal::BuildProposalError;
use crate::metrics::CONSENSUS_L1_GAS_PRICE_PROVIDER_ERROR;
use crate::validate_proposal::ValidateProposalError;

pub(crate) struct StreamSender {
    pub proposal_sender: mpsc::Sender<ProposalPart>,
}

impl StreamSender {
    pub async fn send(&mut self, proposal_part: ProposalPart) -> Result<(), mpsc::SendError> {
        self.proposal_sender.send(proposal_part).await
    }
}

#[derive(Debug)]
pub(crate) struct GasPriceParams {
    pub min_l1_gas_price_wei: GasPrice,
    pub max_l1_gas_price_wei: GasPrice,
    pub max_l1_data_gas_price_wei: GasPrice,
    pub min_l1_data_gas_price_wei: GasPrice,
    pub l1_data_gas_price_multiplier: Ratio<u128>,
    pub l1_gas_tip_wei: GasPrice,
    // TODO(guyn): replace these overrides with fri values
    pub override_l1_gas_price_wei: Option<GasPrice>,
    pub override_l1_data_gas_price_wei: Option<GasPrice>,
    pub override_eth_to_fri_rate: Option<u128>,
}

impl From<StateSyncClientError> for BuildProposalError {
    fn from(e: StateSyncClientError) -> Self {
        match e {
            StateSyncClientError::StateSyncError(StateSyncError::BlockNotFound(e)) => {
                BuildProposalError::StateSyncNotReady(e)
            }
            e => BuildProposalError::StateSyncClientError(e.to_string()),
        }
    }
}

impl From<StateSyncClientError> for ValidateProposalError {
    fn from(e: StateSyncClientError) -> Self {
        match e {
            StateSyncClientError::StateSyncError(StateSyncError::BlockNotFound(e)) => {
                ValidateProposalError::StateSyncNotReady(e)
            }
            e => ValidateProposalError::StateSyncClientError(e.to_string()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct L1PricesInFri {
    pub l1_gas_price: GasPrice,
    pub l1_data_gas_price: GasPrice,
}

impl L1PricesInFri {
    pub fn convert_from_wei(
        wei: L1PricesInWei,
        eth_to_fri_rate: u128,
    ) -> Result<Self, StarknetApiError> {
        Ok(Self {
            l1_gas_price: wei.l1_gas_price.wei_to_fri(eth_to_fri_rate)?,
            l1_data_gas_price: wei.l1_data_gas_price.wei_to_fri(eth_to_fri_rate)?,
        })
    }
}

// TODO(guyn): remove this once we no longer use wei anywhere
#[derive(Clone, Debug)]
pub struct L1PricesInWei {
    pub l1_gas_price: GasPrice,
    pub l1_data_gas_price: GasPrice,
}

// Get the L1 gas prices in fri and wei, and the eth to fri rate.
pub(crate) async fn get_l1_prices_in_fri_and_wei_and_conversion_rate(
    l1_gas_price_provider_client: Arc<dyn L1GasPriceProviderClient>,
    timestamp: u64,
    previous_block_info: Option<&ConsensusBlockInfo>,
    gas_price_params: &GasPriceParams,
) -> (L1PricesInFri, L1PricesInWei, u128) {
    // One of these paths should fill the return values:
    // 1. Both L1 gas price and eth/strk rate are Ok, use those.
    // 2. Otherwise, use previous block info.
    // 3. If that isn't available either, use min gas prices and default eth/strk rate.

    // Get the eth to fri rate from the oracle, and the L1 gas price (in wei) from the provider.
    let (eth_to_fri_rate, price_info) = tokio::join!(
        l1_gas_price_provider_client.get_eth_to_fri_rate(timestamp),
        l1_gas_price_provider_client.get_price_info(BlockTimestamp(timestamp))
    );
    if price_info.is_err() {
        warn!("Failed to get l1 gas price from provider: {:?}", price_info);
        CONSENSUS_L1_GAS_PRICE_PROVIDER_ERROR.increment(1);
    }
    if eth_to_fri_rate.is_err() {
        warn!("Failed to get eth to fri rate from oracle: {:?}", eth_to_fri_rate);
    }
    if let (Ok(eth_to_fri_rate), Ok(mut price_info)) = (eth_to_fri_rate, price_info) {
        // Both L1 prices and rate are Ok, so we can use them.
        info!(
            "raw eth_to_fri_rate (from oracle): {eth_to_fri_rate}, raw l1 gas price wei (from \
             provider): {price_info:?}"
        );
        apply_fee_transformations(&mut price_info, gas_price_params);
        let prices_in_wei = L1PricesInWei {
            l1_gas_price: price_info.base_fee_per_gas,
            l1_data_gas_price: price_info.blob_fee,
        };
        // Apply the eth/strk rate to get prices in fri.
        let l1_gas_prices_fri_result =
            L1PricesInFri::convert_from_wei(prices_in_wei.clone(), eth_to_fri_rate);

        // If conversion fails, leave return_value=None to try backup methods.
        if let Ok(prices_in_fri) = l1_gas_prices_fri_result {
            return (prices_in_fri, prices_in_wei, eth_to_fri_rate);
        } else {
            warn!(
                "Failed to convert L1 gas prices to FRI: {:?}",
                l1_gas_prices_fri_result.clone().err()
            );
        }
    }

    // One or both (oracle/provider) have failed to fetch, or failure in conversion, so we need to
    // try to use the previous block info.
    if let Some(block_info) = previous_block_info {
        let prev_l1_gas_price_wei = L1PricesInWei {
            l1_gas_price: block_info.l1_gas_price_wei,
            l1_data_gas_price: block_info.l1_data_gas_price_wei,
        };
        let prev_l1_gas_price = L1PricesInFri {
            l1_gas_price: block_info.l1_gas_price_fri,
            l1_data_gas_price: block_info.l1_data_gas_price_fri,
        };
        // This calculation can panic if gas price is too high, or zero.
        // It can succeed but still give a zero rate if the price ratio is too small.
        let eth_to_fri_rate = calculate_eth_to_fri_rate(block_info);
        if eth_to_fri_rate > 0 {
            info!(
                "Using previous block info: wei prices: {:?}, fri prices: {:?}, eth to fri rate: \
                 {:?}",
                prev_l1_gas_price_wei, prev_l1_gas_price, eth_to_fri_rate
            );
            return (prev_l1_gas_price, prev_l1_gas_price_wei, eth_to_fri_rate);
        } else {
            // Do not use previous block info. Prefer the default values instead.
            warn!(
                "Previous block info: {:?} implies a zero eth to fri rate. Using default values \
                 instead.",
                block_info
            );
        }
    }

    let default_l1_gas_prices_wei = L1PricesInWei {
        l1_gas_price: gas_price_params.min_l1_gas_price_wei,
        l1_data_gas_price: gas_price_params.min_l1_data_gas_price_wei,
    };
    let default_l1_gas_prices_fri =
        L1PricesInFri::convert_from_wei(default_l1_gas_prices_wei.clone(), DEFAULT_ETH_TO_FRI_RATE)
            .expect("Default values should be convertible between wei and fri.");
    info!(
        "Using default values: fri prices: {:?}, wei prices: {:?}, eth to fri rate: {:?}",
        default_l1_gas_prices_fri,
        default_l1_gas_prices_wei.clone(),
        DEFAULT_ETH_TO_FRI_RATE
    );
    (default_l1_gas_prices_fri, default_l1_gas_prices_wei, DEFAULT_ETH_TO_FRI_RATE)
}

// Apply overrides, use the eth/fri rate and return just the fri and wei prices.
pub(crate) async fn get_l1_prices_in_fri_and_wei(
    l1_gas_price_provider_client: Arc<dyn L1GasPriceProviderClient>,
    timestamp: u64,
    previous_block_info: Option<&ConsensusBlockInfo>,
    gas_price_params: &GasPriceParams,
) -> (L1PricesInFri, L1PricesInWei) {
    let mut values = get_l1_prices_in_fri_and_wei_and_conversion_rate(
        l1_gas_price_provider_client,
        timestamp,
        previous_block_info,
        gas_price_params,
    )
    .await;
    // TODO(guyn): replace overrides in wei with overrides in fri.
    // If there is an override to eth/strk rate, L1 gas price, or data gas price, apply it now.
    if let Some(override_value) = gas_price_params.override_eth_to_fri_rate {
        info!("Overriding eth to fri rate to {override_value}");
        values.2 = override_value;
        values.0 = L1PricesInFri::convert_from_wei(values.1.clone(), override_value).unwrap();
    }
    if let Some(override_value) = gas_price_params.override_l1_gas_price_wei {
        info!("Overriding L1 gas price to {override_value} wei");
        values.1.l1_gas_price = override_value;
        values.0.l1_gas_price = override_value.wei_to_fri(values.2).unwrap_or_else(|err| {
            panic!(
                "Override L1 gas price should be small enough to multiply safely by the eth to \
                 fri conversion rate: {err:?}",
            )
        });
    }
    if let Some(override_value) = gas_price_params.override_l1_data_gas_price_wei {
        info!("Overriding L1 data gas price to {override_value} wei");
        values.1.l1_data_gas_price = override_value;
        values.0.l1_data_gas_price = override_value.wei_to_fri(values.2).unwrap_or_else(|e| {
            panic!(
                "Override L1 data gas price should be small enough to multiply safely by the eth \
                 to fri conversion rate: {:?}",
                e
            )
        });
    }
    // Return only the wei and fri prices, dropping the eth to fri rate.
    (values.0, values.1)
}

pub(crate) fn apply_fee_transformations(
    price_info: &mut PriceInfo,
    gas_price_params: &GasPriceParams,
) {
    price_info.base_fee_per_gas = price_info
        .base_fee_per_gas
        .saturating_add(gas_price_params.l1_gas_tip_wei)
        .clamp(gas_price_params.min_l1_gas_price_wei, gas_price_params.max_l1_gas_price_wei);

    price_info.blob_fee = GasPrice(
        (gas_price_params.l1_data_gas_price_multiplier * price_info.blob_fee.0).to_integer(),
    )
    .clamp(gas_price_params.min_l1_data_gas_price_wei, gas_price_params.max_l1_data_gas_price_wei);
}

pub(crate) fn convert_to_sn_api_block_info(
    block_info: &ConsensusBlockInfo,
) -> Result<starknet_api::block::BlockInfo, StarknetApiError> {
    let l1_gas_price_fri = NonzeroGasPrice::new(block_info.l1_gas_price_fri)?;
    let l1_data_gas_price_fri = NonzeroGasPrice::new(block_info.l1_data_gas_price_fri)?;
    let l1_gas_price_wei = NonzeroGasPrice::new(block_info.l1_gas_price_wei)?;
    let l1_data_gas_price_wei = NonzeroGasPrice::new(block_info.l1_data_gas_price_wei)?;
    let l2_gas_price_fri = NonzeroGasPrice::new(block_info.l2_gas_price_fri)?;
    let eth_to_fri_rate = calculate_eth_to_fri_rate(block_info);

    let l2_gas_price_wei =
        NonzeroGasPrice::new(block_info.l2_gas_price_fri.fri_to_wei(eth_to_fri_rate)?)
            .inspect_err(|_| {
                warn!(
                    "L2 gas price in wei is zero! Conversion rate: {eth_to_fri_rate}, L2 gas \
                     price in FRI: {}",
                    block_info.l2_gas_price_fri
                )
            })?;
    Ok(starknet_api::block::BlockInfo {
        block_number: block_info.height,
        block_timestamp: BlockTimestamp(block_info.timestamp),
        sequencer_address: block_info.builder,
        gas_prices: GasPrices {
            strk_gas_prices: GasPriceVector {
                l1_gas_price: l1_gas_price_fri,
                l1_data_gas_price: l1_data_gas_price_fri,
                l2_gas_price: l2_gas_price_fri,
            },
            eth_gas_prices: GasPriceVector {
                l1_gas_price: l1_gas_price_wei,
                l1_data_gas_price: l1_data_gas_price_wei,
                l2_gas_price: l2_gas_price_wei,
            },
        },
        use_kzg_da: block_info.l1_da_mode.is_use_kzg_da(),
        // TODO(Shahak): Add starknet_version to ConsensusBlockInfo and pass it through here.
        starknet_version: starknet_api::block::StarknetVersion::LATEST,
    })
}

pub(crate) async fn retrospective_block_hash(
    state_sync_client: Arc<dyn StateSyncClient>,
    block_info: &ConsensusBlockInfo,
) -> StateSyncClientResult<Option<BlockHashAndNumber>> {
    let retrospective_block_number = block_info.height.0.checked_sub(STORED_BLOCK_HASH_BUFFER);
    match retrospective_block_number {
        Some(block_number) => {
            let block_number = BlockNumber(block_number);
            let block_hash = state_sync_client.get_block_hash(block_number).await?;
            Ok(Some(BlockHashAndNumber { number: block_number, hash: block_hash }))
        }
        None => {
            info!(
                "Retrospective block number is less than {STORED_BLOCK_HASH_BUFFER}, setting None \
                 as expected."
            );
            Ok(None)
        }
    }
}

pub(crate) async fn wait_for_retrospective_block_hash(
    state_sync_client: Arc<dyn StateSyncClient>,
    block_info: &ConsensusBlockInfo,
    clock: &dyn Clock,
    deadline: DateTime,
    retry_interval: Duration,
) -> StateSyncClientResult<Option<BlockHashAndNumber>> {
    let mut attempts = 0;
    let start_time = clock.now();
    let result = loop {
        attempts += 1;
        let result = retrospective_block_hash(state_sync_client.clone(), block_info).await;

        // If the block is not found, try again after the retry interval. In any other case, return
        // the result.
        match result {
            Err(StateSyncClientError::StateSyncError(StateSyncError::BlockNotFound(_))) => {
                let effective_retry_interval = min(
                    retry_interval,
                    (deadline - clock.now()).to_std().unwrap_or(Duration::ZERO),
                );

                if effective_retry_interval == Duration::ZERO {
                    break result;
                }

                tokio::time::sleep(effective_retry_interval).await;
            }
            _ => break result,
        }
    };

    if attempts > 1 {
        let elapsed_time = clock.now().signed_duration_since(start_time).as_seconds_f32();
        warn!(
            "Multiple attempts ({attempts}) to fetch retrospective block hash. Total time spent: \
             {elapsed_time:.2}s. Last result: {result:?}"
        );
    }

    result
}

pub(crate) fn truncate_to_executed_txs(
    content: &mut Vec<Vec<InternalConsensusTransaction>>,
    final_n_executed_txs: usize,
) -> Vec<Vec<InternalConsensusTransaction>> {
    let content = std::mem::take(content);
    // Truncate `content` to keep only the first `final_n_executed_txs`, preserving batch
    // structure.
    let mut executed_content: Vec<Vec<InternalConsensusTransaction>> = Vec::new();
    let mut remaining = final_n_executed_txs;

    for batch in content {
        if remaining < batch.len() {
            executed_content.push(batch.into_iter().take(remaining).collect());
            break;
        } else {
            remaining -= batch.len();
            executed_content.push(batch);
        }
    }

    executed_content
}

pub(crate) fn make_gas_price_params(config: &ContextConfig) -> GasPriceParams {
    GasPriceParams {
        min_l1_gas_price_wei: GasPrice(config.min_l1_gas_price_wei),
        max_l1_gas_price_wei: GasPrice(config.max_l1_gas_price_wei),
        min_l1_data_gas_price_wei: GasPrice(config.min_l1_data_gas_price_wei),
        max_l1_data_gas_price_wei: GasPrice(config.max_l1_data_gas_price_wei),
        l1_data_gas_price_multiplier: Ratio::new(config.l1_data_gas_price_multiplier_ppt, 1000),
        l1_gas_tip_wei: GasPrice(config.l1_gas_tip_wei),
        override_l1_gas_price_wei: config.override_l1_gas_price_wei.map(GasPrice),
        override_l1_data_gas_price_wei: config.override_l1_data_gas_price_wei.map(GasPrice),
        override_eth_to_fri_rate: config.override_eth_to_fri_rate,
    }
}

fn calculate_eth_to_fri_rate(block_info: &ConsensusBlockInfo) -> u128 {
    block_info
        .l1_gas_price_fri
        .0
        .checked_mul(WEI_PER_ETH)
        .expect("Gas price in Fri should be small enough to multiply by WEI_PER_ETH")
        .checked_div(block_info.l1_gas_price_wei.0)
        .expect("Gas price in Wei should be non-zero")
}
