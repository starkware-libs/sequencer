use std::sync::Arc;

use apollo_l1_gas_price_types::{L1GasPriceProviderClient, PriceInfo, DEFAULT_ETH_TO_FRI_RATE};
use apollo_protobuf::consensus::{ConsensusBlockInfo, ProposalPart};
use apollo_state_sync_types::communication::{
    StateSyncClient,
    StateSyncClientError,
    StateSyncClientResult,
};
use apollo_state_sync_types::errors::StateSyncError;
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
use starknet_api::data_availability::L1DataAvailabilityMode;
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

pub(crate) struct GasPriceParams {
    pub min_l1_gas_price_wei: GasPrice,
    pub max_l1_gas_price_wei: GasPrice,
    pub max_l1_data_gas_price_wei: GasPrice,
    pub min_l1_data_gas_price_wei: GasPrice,
    pub l1_data_gas_price_multiplier: Ratio<u128>,
    pub l1_gas_tip_wei: GasPrice,
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
        use_kzg_da: block_info.l1_da_mode == L1DataAvailabilityMode::Blob,
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
<<<<<<< HEAD
            let block_hash = state_sync_client.get_block_hash(block_number).await?;
            Ok(Some(BlockHashAndNumber { number: block_number, hash: block_hash }))
||||||| 38f03e1d0
            let block = state_sync_client
                // Getting the next block hash because the Sync block only contains parent hash.
                .get_block(block_number.unchecked_next())
                .await
                .map_err(StateSyncError::ClientError)?
                .ok_or(StateSyncError::NotReady(format!(
                "Failed to get retrospective block number {block_number}"
            )))?;
            Some(BlockHashAndNumber {
                number: block_number,
                hash: block.block_header_without_hash.parent_hash,
            })
=======
            let block_hash = state_sync_client.get_block_hash(block_number).await?;
            Some(BlockHashAndNumber { number: block_number, hash: block_hash })
>>>>>>> origin/main-v0.14.0
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
