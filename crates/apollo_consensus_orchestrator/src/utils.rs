#[cfg(test)]
#[path = "utils_test.rs"]
mod utils_test;

use std::cmp::min;
use std::sync::Arc;
use std::time::Duration;

use apollo_batcher_types::communication::{BatcherClient, BatcherClientError};
use apollo_batcher_types::errors::BatcherError;
use apollo_consensus_orchestrator_config::config::ContextDynamicConfig;
use apollo_l1_gas_price_types::{L1GasPriceProviderClient, PriceInfo, DEFAULT_ETH_TO_FRI_RATE};
use apollo_protobuf::consensus::{ProposalInit, ProposalPart};
use apollo_state_sync_types::communication::{SharedStateSyncClient, StateSyncClientError};
use apollo_state_sync_types::errors::StateSyncError;
use apollo_time::time::{Clock, DateTime};
// TODO(Gilad): Define in consensus, either pass to blockifier as config or keep the dup.
use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
use futures::channel::mpsc;
use futures::SinkExt;
use num_rational::Ratio;
use starknet_api::block::{
    BlockHash,
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

use crate::metrics::{
    CONSENSUS_L1_GAS_PRICE_PROVIDER_ERROR,
    CONSENSUS_RETROSPECTIVE_BLOCK_HASH_MISMATCH,
};

pub(crate) struct StreamSender {
    pub proposal_sender: mpsc::Sender<ProposalPart>,
}

impl StreamSender {
    pub async fn send(&mut self, proposal_part: ProposalPart) -> Result<(), mpsc::SendError> {
        self.proposal_sender.send(proposal_part).await
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum RetrospectiveBlockHashError {
    #[error(transparent)]
    StateSyncError(#[from] StateSyncClientError),
    #[error(transparent)]
    BatcherError(#[from] BatcherClientError),
    #[error(
        "Block hash mismatch for block {block_number}. State sync block hash: \
         {state_sync_block_hash:?}, batcher block hash: {batcher_block_hash:?}"
    )]
    HashMismatch {
        block_number: BlockNumber,
        state_sync_block_hash: BlockHash,
        batcher_block_hash: BlockHash,
    },
}

pub(crate) type RetrospectiveBlockHashResult<T> = Result<T, RetrospectiveBlockHashError>;

#[derive(Debug)]
pub(crate) struct GasPriceParams {
    pub min_l1_gas_price_wei: GasPrice,
    pub max_l1_gas_price_wei: GasPrice,
    pub max_l1_data_gas_price_wei: GasPrice,
    pub min_l1_data_gas_price_wei: GasPrice,
    pub l1_data_gas_price_multiplier: Ratio<u128>,
    pub l1_gas_tip_wei: GasPrice,
    pub override_l1_gas_price_fri: Option<GasPrice>,
    pub override_l1_data_gas_price_fri: Option<GasPrice>,
    pub override_eth_to_fri_rate: Option<u128>,
}

#[derive(Clone, Debug)]
pub(crate) struct L1PricesInFri {
    pub l1_gas_price: GasPrice,
    pub l1_data_gas_price: GasPrice,
}

/// Contains only the necessary fields from the previous ProposalInit needed for building/validating
/// proposals. This is a minimal representation to avoid storing the full ProposalInit.
#[derive(Clone, Debug)]
pub(crate) struct PreviousProposalInitInfo {
    pub timestamp: u64,
    pub l1_prices_wei: L1PricesInWei,
    pub l1_prices_fri: L1PricesInFri,
}

impl From<&ProposalInit> for PreviousProposalInitInfo {
    fn from(init: &ProposalInit) -> Self {
        Self {
            timestamp: init.timestamp,
            l1_prices_wei: L1PricesInWei {
                l1_gas_price: init.l1_gas_price_wei,
                l1_data_gas_price: init.l1_data_gas_price_wei,
            },
            l1_prices_fri: L1PricesInFri {
                l1_gas_price: init.l1_gas_price_fri,
                l1_data_gas_price: init.l1_data_gas_price_fri,
            },
        }
    }
}

impl L1PricesInFri {
    pub(crate) fn convert_from_wei(
        wei: &L1PricesInWei,
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
pub(crate) struct L1PricesInWei {
    pub l1_gas_price: GasPrice,
    pub l1_data_gas_price: GasPrice,
}

// Get the L1 gas prices in fri and wei, and the eth to fri rate.
pub(crate) async fn get_l1_prices_in_fri_and_wei_and_conversion_rate(
    l1_gas_price_provider_client: Arc<dyn L1GasPriceProviderClient>,
    timestamp: u64,
    previous_proposal_init: Option<&PreviousProposalInitInfo>,
    gas_price_params: &GasPriceParams,
) -> (L1PricesInFri, L1PricesInWei, u128) {
    // One of these paths should fill the return values:
    // 1. Both L1 gas price and eth/strk rate are Ok, use those.
    // 2. Otherwise, use previous block info.
    // 3. If that isn't available either, use min gas prices and default eth/strk rate.

    // Get the eth to fri rate from the oracle, and the L1 gas price (in wei) from the provider.
    let (eth_to_fri_rate, price_info) = tokio::join!(
        l1_gas_price_provider_client.get_rate(timestamp),
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
            L1PricesInFri::convert_from_wei(&prices_in_wei, eth_to_fri_rate);
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
    if let Some(prev_info) = previous_proposal_init {
        let prev_l1_gas_price_wei = prev_info.l1_prices_wei.clone();
        let prev_l1_gas_price = prev_info.l1_prices_fri.clone();
        // This calculation can fail if gas price is too high, or zero, or if the prices cause the
        // rate to be zero.
        let eth_to_fri_rate = calculate_eth_to_fri_rate(prev_info);
        match eth_to_fri_rate {
            Ok(eth_to_fri_rate) => {
                info!(
                    "Using previous block info: wei prices: {:?}, fri prices: {:?}, eth to fri \
                     rate: {:?}",
                    prev_l1_gas_price_wei, prev_l1_gas_price, eth_to_fri_rate
                );
                return (prev_l1_gas_price, prev_l1_gas_price_wei, eth_to_fri_rate);
            }
            Err(error) => {
                warn!(
                    "Error calculating eth to fri rate from previous block info: {:?}: {:?}",
                    prev_info, error
                );
                // Do not use previous block info. Prefer the default values instead.
            }
        }
    }

    let default_l1_gas_prices_wei = L1PricesInWei {
        l1_gas_price: gas_price_params.min_l1_gas_price_wei,
        l1_data_gas_price: gas_price_params.min_l1_data_gas_price_wei,
    };
    let default_l1_gas_prices_fri =
        L1PricesInFri::convert_from_wei(&default_l1_gas_prices_wei, DEFAULT_ETH_TO_FRI_RATE)
            .expect("Default values should be convertible between wei and fri.");
    info!(
        "Using default values: fri prices: {:?}, wei prices: {:?}, eth to fri rate: {:?}",
        default_l1_gas_prices_fri, default_l1_gas_prices_wei, DEFAULT_ETH_TO_FRI_RATE
    );
    (default_l1_gas_prices_fri, default_l1_gas_prices_wei, DEFAULT_ETH_TO_FRI_RATE)
}

// Apply overrides, use the eth/fri rate and return just the fri and wei prices.
pub(crate) async fn get_l1_prices_in_fri_and_wei(
    l1_gas_price_provider_client: Arc<dyn L1GasPriceProviderClient>,
    timestamp: u64,
    previous_proposal_init: Option<&PreviousProposalInitInfo>,
    gas_price_params: &GasPriceParams,
) -> (L1PricesInFri, L1PricesInWei) {
    let mut values = get_l1_prices_in_fri_and_wei_and_conversion_rate(
        l1_gas_price_provider_client,
        timestamp,
        previous_proposal_init,
        gas_price_params,
    )
    .await;
    // If there is an override to eth/strk rate, L1 gas price, or data gas price, apply it now.
    // If we also override the L1 data gas price, we will have to recalculate the new prices in wei,
    // using the new eth to fri rate. If we do not override anything (the default) we shouldn't have
    // to recalculate anything.
    if let Some(override_value) = gas_price_params.override_eth_to_fri_rate {
        info!("Overriding eth to fri rate to {override_value}");
        values.2 = override_value;
        values.0 = L1PricesInFri::convert_from_wei(&values.1, override_value)
            .unwrap_or_else(|err| panic!("Failed to convert L1 prices to FRI: {err:?}"));
    }
    if let Some(override_value) = gas_price_params.override_l1_gas_price_fri {
        info!("Overriding L1 gas price to {override_value} fri");
        values.0.l1_gas_price = override_value;
        values.1.l1_gas_price = override_value.fri_to_wei(values.2).unwrap_or_else(|err| {
            panic!(
                "Override L1 gas price should be small enough to multiply safely by the eth to \
                 wei factor (10^18), and divide safely by the (non-zero) eth to fri rate: {err:?}",
            )
        });
    }
    if let Some(override_value) = gas_price_params.override_l1_data_gas_price_fri {
        info!("Overriding L1 data gas price to {override_value} fri");
        values.0.l1_data_gas_price = override_value;
        values.1.l1_data_gas_price = override_value.fri_to_wei(values.2).unwrap_or_else(|e| {
            panic!(
                "Override L1 data gas price should be small enough to multiply safely by the eth \
                 to wei factor (10^18), and divide safely by the (non-zero) eth to fri rate: {e:?}",
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
    init: &ProposalInit,
) -> Result<starknet_api::block::BlockInfo, StarknetApiError> {
    if init.l1_gas_price_fri.0 == 0
        || init.l1_gas_price_wei.0 == 0
        || init.l1_data_gas_price_fri.0 == 0
        || init.l1_data_gas_price_wei.0 == 0
        || init.l2_gas_price_fri.0 == 0
    {
        warn!("Zero gas price detected in block info: {:?}", init);
    }

    let l1_gas_price_fri = NonzeroGasPrice::new(init.l1_gas_price_fri)?;
    let l1_data_gas_price_fri = NonzeroGasPrice::new(init.l1_data_gas_price_fri)?;
    let l1_gas_price_wei = NonzeroGasPrice::new(init.l1_gas_price_wei)?;
    let l1_data_gas_price_wei = NonzeroGasPrice::new(init.l1_data_gas_price_wei)?;
    let l2_gas_price_fri = NonzeroGasPrice::new(init.l2_gas_price_fri)?;
    let proposal_init_info = PreviousProposalInitInfo::from(init);
    let eth_to_fri_rate = calculate_eth_to_fri_rate(&proposal_init_info)?;

    let l2_gas_price_wei = NonzeroGasPrice::new(init.l2_gas_price_fri.fri_to_wei(eth_to_fri_rate)?)
        .inspect_err(|_| {
            warn!(
                "L2 gas price in wei is zero! Conversion rate: {eth_to_fri_rate}, L2 gas price in \
                 FRI: {}",
                init.l2_gas_price_fri
            )
        })?;
    Ok(starknet_api::block::BlockInfo {
        block_number: init.height,
        block_timestamp: BlockTimestamp(init.timestamp),
        sequencer_address: init.builder,
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
        use_kzg_da: init.l1_da_mode.is_use_kzg_da(),
        starknet_version: init.starknet_version,
    })
}

/// Get the block hash for the retrospective block from the batcher and state sync, and return a
/// valid result only if the values are equal.
/// Also verifies that the batcher is done calculating the block hash of the block after the
/// retrospective to avoid a deadlock that can happen if all nodes only report the retrospective
/// block hash and not the block hashes of more advanced blocks.
pub(crate) async fn retrospective_block_hash(
    batcher_client: Arc<dyn BatcherClient>,
    state_sync_client: SharedStateSyncClient,
    init: &ProposalInit,
    compare_retrospective_block_hash: bool,
) -> RetrospectiveBlockHashResult<Option<BlockHashAndNumber>> {
    if let Some(required_height) = (init.height.0 + 1).checked_sub(STORED_BLOCK_HASH_BUFFER) {
        // Just verify that the batcher has done calculating it.
        batcher_client.get_block_hash(BlockNumber(required_height)).await?;
    }

    let retrospective_block_number = init.height.0.checked_sub(STORED_BLOCK_HASH_BUFFER);

    let Some(block_number) = retrospective_block_number else {
        info!(
            "Retrospective block number is less than {STORED_BLOCK_HASH_BUFFER}, setting None as \
             expected."
        );
        return Ok(None);
    };

    let block_number = BlockNumber(block_number);

    // First try from state sync - assuming it takes longer to this one to be ready.
    let state_sync_block_hash = state_sync_client.get_block_hash(block_number).await?;

    // Then try from batcher.
    let batcher_block_hash = batcher_client.get_block_hash(block_number).await?;

    if compare_retrospective_block_hash && state_sync_block_hash != batcher_block_hash {
        warn!(
            "Retrospective block hashes mismatch for block {block_number}: state sync block hash: \
             {state_sync_block_hash:?}, batcher block hash: {batcher_block_hash:?}"
        );
        CONSENSUS_RETROSPECTIVE_BLOCK_HASH_MISMATCH.increment(1);
        return Err(RetrospectiveBlockHashError::HashMismatch {
            block_number,
            state_sync_block_hash,
            batcher_block_hash,
        });
    }
    Ok(Some(BlockHashAndNumber { number: block_number, hash: batcher_block_hash }))
}

pub(crate) async fn wait_for_retrospective_block_hash(
    batcher_client: Arc<dyn BatcherClient>,
    state_sync_client: SharedStateSyncClient,
    init: &ProposalInit,
    clock: &dyn Clock,
    deadline: DateTime,
    retry_interval: Duration,
    compare_retrospective_block_hash: bool,
) -> RetrospectiveBlockHashResult<Option<BlockHashAndNumber>> {
    let mut attempts = 0;
    let start_time = clock.now();
    let result = loop {
        attempts += 1;
        let result = retrospective_block_hash(
            batcher_client.clone(),
            state_sync_client.clone(),
            init,
            compare_retrospective_block_hash,
        )
        .await;

        // If the block is not found, try again after the retry interval. In any other case, return
        // the result.
        let state_sync_not_ready = matches!(
            result,
            Err(RetrospectiveBlockHashError::StateSyncError(StateSyncClientError::StateSyncError(
                StateSyncError::BlockNotFound(_)
            )))
        );
        let batcher_not_ready = matches!(
            result,
            Err(RetrospectiveBlockHashError::BatcherError(BatcherClientError::BatcherError(
                BatcherError::BlockHashNotFound(_)
            )))
        );

        if !state_sync_not_ready && !batcher_not_ready {
            break result;
        }

        let effective_retry_interval =
            min(retry_interval, (deadline - clock.now()).to_std().unwrap_or(Duration::ZERO));

        if effective_retry_interval == Duration::ZERO {
            break result;
        } else {
            let not_ready_client = if state_sync_not_ready { "State Sync" } else { "Batcher" };
            warn!(
                "Attempt to retrieve retrospective block hash failed. {not_ready_client} is not \
                 ready. \nRetrying in {effective_retry_interval:?}."
            );
        }

        tokio::time::sleep(effective_retry_interval).await;
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
    let mut remaining_tx_count = final_n_executed_txs;

    for batch in content {
        if remaining_tx_count < batch.len() {
            executed_content.push(batch.into_iter().take(remaining_tx_count).collect());
            break;
        } else {
            remaining_tx_count -= batch.len();
            executed_content.push(batch);
        }
    }

    executed_content
}

pub(crate) fn make_gas_price_params(config: &ContextDynamicConfig) -> GasPriceParams {
    GasPriceParams {
        min_l1_gas_price_wei: GasPrice(config.min_l1_gas_price_wei),
        max_l1_gas_price_wei: GasPrice(config.max_l1_gas_price_wei),
        min_l1_data_gas_price_wei: GasPrice(config.min_l1_data_gas_price_wei),
        max_l1_data_gas_price_wei: GasPrice(config.max_l1_data_gas_price_wei),
        l1_data_gas_price_multiplier: Ratio::new(config.l1_data_gas_price_multiplier_ppt, 1000),
        l1_gas_tip_wei: GasPrice(config.l1_gas_tip_wei),
        override_l1_gas_price_fri: config.override_l1_gas_price_fri.map(GasPrice),
        override_l1_data_gas_price_fri: config.override_l1_data_gas_price_fri.map(GasPrice),
        override_eth_to_fri_rate: config.override_eth_to_fri_rate,
    }
}

fn calculate_eth_to_fri_rate(
    proposal_init_info: &PreviousProposalInitInfo,
) -> Result<u128, StarknetApiError> {
    let eth_to_fri_rate = proposal_init_info
        .l1_prices_fri
        .l1_gas_price
        .0
        .checked_mul(WEI_PER_ETH)
        .ok_or_else(|| {
            StarknetApiError::GasPriceConversionError(format!(
                "Gas price in Fri should be small enough to multiply by WEI_PER_ETH. Previous \
                 proposal init info: {:?}",
                proposal_init_info
            ))
        })?
        .checked_div(proposal_init_info.l1_prices_wei.l1_gas_price.0)
        .ok_or_else(|| {
            StarknetApiError::GasPriceConversionError(format!(
                "Gas price in Wei should be non-zero. Previous proposal init info: {:?}",
                proposal_init_info
            ))
        })?;
    if eth_to_fri_rate == 0 {
        return Err(StarknetApiError::GasPriceConversionError(format!(
            "Eth to fri rate is zero. Previous proposal init info: {:?}",
            proposal_init_info
        )));
    }
    Ok(eth_to_fri_rate)
}
