use std::cmp::min;
use std::sync::Arc;
use std::time::Duration;

use apollo_batcher_types::communication::{BatcherClient, BatcherClientError};
use apollo_batcher_types::errors::BatcherError;
use apollo_consensus_orchestrator_config::config::ContextConfig;
use apollo_l1_gas_price_types::{L1GasPriceProviderClient, PriceInfo, DEFAULT_ETH_TO_FRI_RATE};
use apollo_protobuf::consensus::{ConsensusBlockInfo, ProposalPart};
use apollo_state_sync_types::communication::{StateSyncClient, StateSyncClientError};
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
};
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::StarknetApiError;
use tracing::{info, warn};

use crate::metrics::CONSENSUS_L1_GAS_PRICE_PROVIDER_ERROR;

#[derive(Debug, thiserror::Error)]
pub(crate) enum ClientsError {
    #[error("Batcher error: {batcher_error:?}, State sync error: {state_sync_error:?}")]
    Both { batcher_error: BatcherClientError, state_sync_error: StateSyncClientError },
    #[error("Batcher and/or state sync are not ready: block number {0} not found.")]
    NotReady(BlockNumber),
}

pub(crate) type ClientsResult<T> = Result<T, ClientsError>;

impl ClientsError {
    pub(crate) fn from_errors(
        batcher_error: BatcherClientError,
        state_sync_error: StateSyncClientError,
    ) -> Self {
        match (batcher_error, state_sync_error) {
            (
                BatcherClientError::BatcherError(BatcherError::BlockHashNotFound(block_number)),
                _,
            )
            | (
                _,
                StateSyncClientError::StateSyncError(StateSyncError::BlockNotFound(block_number)),
            ) => ClientsError::NotReady(block_number),
            (batcher_error, state_sync_error) => {
                ClientsError::Both { batcher_error, state_sync_error }
            }
        }
    }
}

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
    pub override_l1_gas_price_wei: Option<GasPrice>,
    pub override_l1_data_gas_price_wei: Option<GasPrice>,
    pub override_eth_to_fri_rate: Option<u128>,
}

pub(crate) async fn get_oracle_rate_and_prices(
    l1_gas_price_provider_client: Arc<dyn L1GasPriceProviderClient>,
    timestamp: u64,
    previous_block_info: Option<&ConsensusBlockInfo>,
    gas_price_params: &GasPriceParams,
) -> (u128, PriceInfo) {
    let (eth_to_strk_rate, price_info) = tokio::join!(
        l1_gas_price_provider_client.get_eth_to_fri_rate(timestamp),
        l1_gas_price_provider_client.get_price_info(BlockTimestamp(timestamp))
    );
    let mut return_values;

    if price_info.is_err() {
        warn!("Failed to get l1 gas price from provider: {:?}", price_info);
        CONSENSUS_L1_GAS_PRICE_PROVIDER_ERROR.increment(1);
    }
    if eth_to_strk_rate.is_err() {
        warn!("Failed to get eth to strk rate from oracle: {:?}", eth_to_strk_rate);
    }

    if let (Ok(eth_to_strk_rate), Ok(mut price_info)) = (eth_to_strk_rate, price_info) {
        // Both L1 prices and rate are Ok, so we can use them.
        info!(
            "raw eth_to_strk_rate (from oracle): {eth_to_strk_rate}, raw l1 gas price wei (from \
             provider): {price_info:?}"
        );
        apply_fee_transformations(&mut price_info, gas_price_params);
        return_values = (eth_to_strk_rate, price_info);
    } else {
        // One or both have failed, need to use previous block info (or default values)
        match previous_block_info {
            Some(block_info) => {
                let prev_l1_gas_price = PriceInfo {
                    base_fee_per_gas: block_info.l1_gas_price_wei,
                    blob_fee: block_info.l1_data_gas_price_wei,
                };
                info!(
                    "Using values from previous block info. eth_to_strk_rate: {}, l1 gas price: \
                     {:?}",
                    block_info.eth_to_fri_rate, prev_l1_gas_price
                );
                return_values = (block_info.eth_to_fri_rate, prev_l1_gas_price);
            }
            None => {
                let l1_gas_price = PriceInfo {
                    base_fee_per_gas: gas_price_params.min_l1_gas_price_wei,
                    blob_fee: gas_price_params.min_l1_data_gas_price_wei,
                };
                info!(
                    "No previous block info available, using default values. eth_to_strk_rate: \
                     {}, l1 gas price: {:?}",
                    DEFAULT_ETH_TO_FRI_RATE, l1_gas_price
                );
                return_values = (DEFAULT_ETH_TO_FRI_RATE, l1_gas_price);
            }
        }
    }

    // If there is an override to L1 gas price or data gas price, apply it here:
    if let Some(override_value) = gas_price_params.override_l1_gas_price_wei {
        info!("Overriding L1 gas price to {override_value} wei");
        return_values.1.base_fee_per_gas = override_value;
    }
    if let Some(override_value) = gas_price_params.override_l1_data_gas_price_wei {
        info!("Overriding L1 data gas price to {override_value} wei");
        return_values.1.blob_fee = override_value;
    }

    if let Some(override_value) = gas_price_params.override_eth_to_fri_rate {
        info!("Overriding conversion rate to {override_value}");
        return_values.0 = override_value;
    }

    return_values
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
    let l1_gas_price_fri =
        NonzeroGasPrice::new(block_info.l1_gas_price_wei.wei_to_fri(block_info.eth_to_fri_rate)?)?;
    let l1_data_gas_price_fri = NonzeroGasPrice::new(
        block_info.l1_data_gas_price_wei.wei_to_fri(block_info.eth_to_fri_rate)?,
    )?;
    let l2_gas_price_fri = NonzeroGasPrice::new(block_info.l2_gas_price_fri)?;
    let l2_gas_price_wei =
        NonzeroGasPrice::new(block_info.l2_gas_price_fri.fri_to_wei(block_info.eth_to_fri_rate)?)?;
    let l1_gas_price_wei = NonzeroGasPrice::new(block_info.l1_gas_price_wei)?;
    let l1_data_gas_price_wei = NonzeroGasPrice::new(block_info.l1_data_gas_price_wei)?;

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

/// Get the block hash for the retrospective block.
/// First try to get the block hash from the batcher. If that fails, fall back to state sync.
pub(crate) async fn retrospective_block_hash(
    batcher_client: &dyn BatcherClient,
    state_sync_client: Arc<dyn StateSyncClient>,
    block_info: &ConsensusBlockInfo,
) -> ClientsResult<Option<BlockHashAndNumber>> {
    let retrospective_block_number = block_info.height.0.checked_sub(STORED_BLOCK_HASH_BUFFER);
    match retrospective_block_number {
        Some(block_number) => {
            let block_number = BlockNumber(block_number);
            let block_hash = match batcher_client.get_block_hash(block_number).await {
                Ok(block_hash) => block_hash,
                Err(batcher_error) => {
                    let block_hash = state_sync_client.get_block_hash(block_number).await.map_err(
                        |state_sync_error| {
                            ClientsError::from_errors(batcher_error.clone(), state_sync_error)
                        },
                    )?;
                    // TODO(Rotem): Add a metric to track if we fall back to state sync and it
                    // succeeds.
                    warn!(
                        "Failed to get block hash for block {block_number} from batcher, fell \
                         back to state sync and succeeded. Error: {batcher_error:?}"
                    );
                    block_hash
                }
            };
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

// TODO(Rotem): When we trust the batcher, we can move this function to the batcher and get the
// retrospective block hash via the batcher client.
pub(crate) async fn wait_for_retrospective_block_hash(
    batcher_client: &dyn BatcherClient,
    state_sync_client: Arc<dyn StateSyncClient>,
    block_info: &ConsensusBlockInfo,
    clock: &dyn Clock,
    deadline: DateTime,
    retry_interval: Duration,
) -> ClientsResult<Option<BlockHashAndNumber>> {
    let mut attempts = 0;
    let start_time = clock.now();
    let result = loop {
        attempts += 1;
        let result =
            retrospective_block_hash(batcher_client, state_sync_client.clone(), block_info).await;

        // If the block is not found, try again after the retry interval. In any other case, return
        // the result.
        match result {
            Err(ClientsError::NotReady(_)) => {
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
