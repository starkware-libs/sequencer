#[cfg(test)]
#[path = "validate_proposal_test.rs"]
mod validate_proposal_test;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use apollo_batcher_types::batcher_types::{
    ProposalId,
    ProposalStatus,
    SendProposalContent,
    SendProposalContentInput,
    ValidateBlockInput,
};
use apollo_batcher_types::communication::{BatcherClient, BatcherClientError};
use apollo_class_manager_types::transaction_converter::TransactionConverterTrait;
use apollo_consensus::types::ProposalCommitment;
use apollo_l1_gas_price_types::errors::{EthToStrkOracleClientError, L1GasPriceClientError};
use apollo_l1_gas_price_types::{EthToStrkOracleClientTrait, L1GasPriceProviderClient};
use apollo_protobuf::consensus::{ConsensusBlockInfo, ProposalFin, ProposalPart, TransactionBatch};
use apollo_state_sync_types::communication::{StateSyncClient, StateSyncClientError};
use apollo_time::time::{sleep_until, Clock, DateTime};
use futures::channel::mpsc;
use futures::StreamExt;
use starknet_api::block::{BlockHash, BlockNumber, GasPrice};
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::transaction::TransactionHash;
use starknet_api::StarknetApiError;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn};

use crate::metrics::{
    CONSENSUS_L1_DATA_GAS_MISMATCH,
    CONSENSUS_L1_GAS_MISMATCH,
    CONSENSUS_NUM_BATCHES_IN_PROPOSAL,
    CONSENSUS_NUM_TXS_IN_PROPOSAL,
};
use crate::orchestrator_versioned_constants::VersionedConstants;
use crate::sequencer_consensus_context::{BuiltProposals, SequencerConsensusContextDeps};
use crate::utils::{
    convert_to_sn_api_block_info,
    get_oracle_rate_and_prices,
    retrospective_block_hash,
    truncate_to_executed_txs,
    GasPriceParams,
};

pub(crate) struct ProposalValidateArguments {
    pub deps: SequencerConsensusContextDeps,
    pub block_info_validation: BlockInfoValidation,
    pub proposal_id: ProposalId,
    pub timeout: Duration,
    pub batcher_timeout_margin: Duration,
    pub valid_proposals: Arc<Mutex<BuiltProposals>>,
    pub content_receiver: mpsc::Receiver<ProposalPart>,
    pub gas_price_params: GasPriceParams,
    pub cancel_token: CancellationToken,
}

// Contains parameters required for validating block info.
#[derive(Clone, Debug)]
pub(crate) struct BlockInfoValidation {
    pub height: BlockNumber,
    pub block_timestamp_window_seconds: u64,
    pub previous_block_info: Option<ConsensusBlockInfo>,
    pub l1_da_mode: L1DataAvailabilityMode,
    pub l2_gas_price_fri: GasPrice,
}

enum HandledProposalPart {
    Continue,
    Invalid,
    Finished(ProposalCommitment, ProposalFin),
    Failed(String),
}

enum SecondProposalPart {
    BlockInfo(ConsensusBlockInfo),
    Fin(ProposalFin),
}

type ValidateProposalResult<T> = Result<T, ValidateProposalError>;

#[derive(Debug, thiserror::Error)]
pub(crate) enum ValidateProposalError {
    #[error("Batcher error: {0}")]
    Batcher(String, BatcherClientError),
    #[error("State sync client error: {0}")]
    StateSyncClientError(#[from] StateSyncClientError),
    #[error("State sync is not ready: {0}")]
    StateSyncNotReady(String),
    // Consensus may exit early (e.g. sync).
    #[error("Failed to send commitment to consensus: {0}")]
    SendError(ProposalCommitment),
    #[error("EthToStrkOracle error: {0}")]
    EthToStrkOracle(#[from] EthToStrkOracleClientError),
    #[error("L1GasPriceProvider error: {0}")]
    L1GasPriceProvider(#[from] L1GasPriceClientError),
    #[error("Block info conversion error: {0}")]
    BlockInfoConversion(#[from] StarknetApiError),
    #[error("Invalid BlockInfo: {2}. received:{0:?}, validation criteria {1:?}.")]
    InvalidBlockInfo(ConsensusBlockInfo, BlockInfoValidation, String),
    #[error("Validation timed out while {0}")]
    ValidationTimeout(String),
    #[error("Proposal interrupted while {0}")]
    ProposalInterrupted(String),
    #[error("Got an invalid second proposal part: {0:?}.")]
    InvalidSecondProposalPart(Option<ProposalPart>),
    #[error("Batcher returned Invalid status.")]
    InvalidProposal,
    #[error("Proposal part {1:?} failed validation: {0}.")]
    ProposalPartFailed(String, Option<ProposalPart>),
    #[error("proposal_commitment built by the batcher does not match the proposal fin.")]
    ProposalFinMismatch,
    #[error("Cannot calculate deadline. timeout: {timeout:?}, now: {now:?}")]
    CannotCalculateDeadline { timeout: Duration, now: DateTime },
}

pub(crate) async fn validate_proposal(
    mut args: ProposalValidateArguments,
) -> ValidateProposalResult<ProposalCommitment> {
    let mut content = Vec::new();
    let mut final_n_executed_txs: Option<usize> = None;
    let now = args.deps.clock.now();

    let Some(deadline) = now.checked_add_signed(chrono::TimeDelta::from_std(args.timeout).unwrap())
    else {
        return Err(ValidateProposalError::CannotCalculateDeadline { timeout: args.timeout, now });
    };

    let block_info = match await_second_proposal_part(
        &args.cancel_token,
        deadline,
        &mut args.content_receiver,
        args.deps.clock.as_ref(),
    )
    .await?
    {
        SecondProposalPart::BlockInfo(block_info) => block_info,
        SecondProposalPart::Fin(ProposalFin { proposal_commitment }) => {
            return Ok(proposal_commitment);
        }
    };
    is_block_info_valid(
        args.block_info_validation.clone(),
        block_info.clone(),
        args.deps.eth_to_strk_oracle_client,
        args.deps.clock.as_ref(),
        args.deps.l1_gas_price_provider,
        &args.gas_price_params,
    )
    .await?;

    initiate_validation(
        args.deps.batcher.as_ref(),
        args.deps.state_sync_client,
        block_info.clone(),
        args.proposal_id,
        args.timeout + args.batcher_timeout_margin,
        args.deps.clock.as_ref(),
    )
    .await?;

    // Validating the rest of the proposal parts.
    let (built_block, received_fin) = loop {
        tokio::select! {
            _ = args.cancel_token.cancelled() => {
                batcher_abort_proposal(args.deps.batcher.as_ref(), args.proposal_id).await;
                return Err(ValidateProposalError::ProposalInterrupted(
                    "validating proposal parts".to_string(),
                ));
            }
            _ = sleep_until(deadline, args.deps.clock.as_ref()) => {
                batcher_abort_proposal(args.deps.batcher.as_ref(), args.proposal_id).await;
                return Err(ValidateProposalError::ValidationTimeout(
                    "validating proposal parts".to_string(),
                ));
            }
            proposal_part = args.content_receiver.next() => {
                match handle_proposal_part(
                    args.proposal_id,
                    args.deps.batcher.as_ref(),
                    proposal_part.clone(),
                    &mut content,
                    &mut final_n_executed_txs,
                    args.deps.transaction_converter.clone(),
                ).await {
                    HandledProposalPart::Finished(built_block, received_fin) => {
                        break (built_block, received_fin);
                    }
                    HandledProposalPart::Continue => {continue;}
                    HandledProposalPart::Invalid => {
                        // No need to abort since the Batcher is the source of this info.
                        return Err(ValidateProposalError::InvalidProposal);
                    }
                    HandledProposalPart::Failed(fail_reason) => {
                        batcher_abort_proposal(args.deps.batcher.as_ref(), args.proposal_id).await;
                        return Err(ValidateProposalError::ProposalPartFailed(fail_reason,proposal_part));
                    }
                }
            }
        }
    };

    let n_executed_txs = content.iter().map(|batch| batch.len()).sum::<usize>();
    CONSENSUS_NUM_BATCHES_IN_PROPOSAL.set_lossy(content.len());
    CONSENSUS_NUM_TXS_IN_PROPOSAL.set_lossy(n_executed_txs);

    // Update valid_proposals before sending fin to avoid a race condition
    // with `repropose` being called before `valid_proposals` is updated.
    let mut valid_proposals = args.valid_proposals.lock().unwrap();
    valid_proposals.insert_proposal_for_height(
        &args.block_info_validation.height,
        &built_block,
        block_info,
        content,
        &args.proposal_id,
    );

    // TODO(matan): Switch to signature validation.
    if built_block != received_fin.proposal_commitment {
        return Err(ValidateProposalError::ProposalFinMismatch);
    }

    Ok(built_block)
}

#[instrument(level = "warn", skip_all, fields(?block_info_validation, ?block_info_proposed))]
async fn is_block_info_valid(
    block_info_validation: BlockInfoValidation,
    block_info_proposed: ConsensusBlockInfo,
    eth_to_strk_oracle_client: Arc<dyn EthToStrkOracleClientTrait>,
    clock: &dyn Clock,
    l1_gas_price_provider: Arc<dyn L1GasPriceProviderClient>,
    gas_price_params: &GasPriceParams,
) -> ValidateProposalResult<()> {
    let now: u64 = clock.unix_now();
    let last_block_timestamp =
        block_info_validation.previous_block_info.as_ref().map_or(0, |info| info.timestamp);
    if block_info_proposed.timestamp < last_block_timestamp {
        return Err(ValidateProposalError::InvalidBlockInfo(
            block_info_proposed.clone(),
            block_info_validation.clone(),
            format!(
                "Timestamp is too old: last_block_timestamp={}, proposed={}",
                last_block_timestamp, block_info_proposed.timestamp
            ),
        ));
    }
    if block_info_proposed.timestamp > now + block_info_validation.block_timestamp_window_seconds {
        return Err(ValidateProposalError::InvalidBlockInfo(
            block_info_proposed.clone(),
            block_info_validation.clone(),
            format!(
                "Timestamp is in the future: now={}, block_timestamp_window_seconds={}, \
                 proposed={}",
                now,
                block_info_validation.block_timestamp_window_seconds,
                block_info_proposed.timestamp
            ),
        ));
    }
    if !(block_info_proposed.height == block_info_validation.height
        && block_info_proposed.l1_da_mode == block_info_validation.l1_da_mode
        && block_info_proposed.l2_gas_price_fri == block_info_validation.l2_gas_price_fri)
    {
        return Err(ValidateProposalError::InvalidBlockInfo(
            block_info_proposed.clone(),
            block_info_validation.clone(),
            "Block info validation failed".to_string(),
        ));
    }
    let (eth_to_fri_rate, l1_gas_prices) = get_oracle_rate_and_prices(
        eth_to_strk_oracle_client,
        l1_gas_price_provider,
        block_info_proposed.timestamp,
        block_info_validation.previous_block_info.as_ref(),
        gas_price_params,
    )
    .await;
    let l1_gas_price_margin_percent =
        VersionedConstants::latest_constants().l1_gas_price_margin_percent.into();
    debug!("L1 price info: {l1_gas_prices:?}");

    let l1_gas_price_fri = l1_gas_prices.base_fee_per_gas.wei_to_fri(eth_to_fri_rate)?;
    let l1_data_gas_price_fri = l1_gas_prices.blob_fee.wei_to_fri(eth_to_fri_rate)?;
    let l1_gas_price_fri_proposed =
        block_info_proposed.l1_gas_price_wei.wei_to_fri(block_info_proposed.eth_to_fri_rate)?;
    let l1_data_gas_price_fri_proposed = block_info_proposed
        .l1_data_gas_price_wei
        .wei_to_fri(block_info_proposed.eth_to_fri_rate)?;

    if !(within_margin(l1_gas_price_fri_proposed, l1_gas_price_fri, l1_gas_price_margin_percent)
        && within_margin(
            l1_data_gas_price_fri_proposed,
            l1_data_gas_price_fri,
            l1_gas_price_margin_percent,
        ))
    {
        return Err(ValidateProposalError::InvalidBlockInfo(
            block_info_proposed,
            block_info_validation,
            format!(
                "L1 gas price mismatch: expected L1 gas price FRI={}, proposed={}, expected L1 \
                 data gas price FRI={}, proposed={}, l1_gas_price_margin_percent={}",
                l1_gas_price_fri,
                l1_gas_price_fri_proposed,
                l1_data_gas_price_fri,
                l1_data_gas_price_fri_proposed,
                l1_gas_price_margin_percent
            ),
        ));
    }
    // TODO(Asmaa): consider removing after 0.14 as other validators may use other sources.
    if l1_gas_price_fri_proposed != l1_gas_price_fri {
        CONSENSUS_L1_GAS_MISMATCH.increment(1);
    }
    if l1_data_gas_price_fri_proposed != l1_data_gas_price_fri {
        CONSENSUS_L1_DATA_GAS_MISMATCH.increment(1);
    }
    Ok(())
}

fn within_margin(number1: GasPrice, number2: GasPrice, margin_percent: u128) -> bool {
    let margin = (number1.0 * margin_percent) / 100;
    number1.0.abs_diff(number2.0) <= margin
}

// The second proposal part when validating a proposal must be:
// 1. Fin - empty proposal.
// 2. BlockInfo - required to begin executing TX batches.
async fn await_second_proposal_part(
    cancel_token: &CancellationToken,
    deadline: DateTime,
    content_receiver: &mut mpsc::Receiver<ProposalPart>,
    clock: &dyn Clock,
) -> ValidateProposalResult<SecondProposalPart> {
    tokio::select! {
        _ = cancel_token.cancelled() => {
            Err(ValidateProposalError::ProposalInterrupted(
                "waiting for second proposal part".to_string(),
            ))
        }
        _ = sleep_until(deadline, clock) => {
            Err(ValidateProposalError::ValidationTimeout(
                "waiting for second proposal part".to_string(),
            ))
        }
        proposal_part = content_receiver.next() => {
            match proposal_part {
                Some(ProposalPart::BlockInfo(block_info)) => {
                    Ok(SecondProposalPart::BlockInfo(block_info))
                }
                Some(ProposalPart::Fin(ProposalFin { proposal_commitment })) => {
                    warn!("Received an empty proposal.");
                    Ok(SecondProposalPart::Fin(ProposalFin { proposal_commitment }))
                }
                x => {
                    Err(ValidateProposalError::InvalidSecondProposalPart(x
                    ))
                }
            }
        }
    }
}

async fn initiate_validation(
    batcher: &dyn BatcherClient,
    state_sync_client: Arc<dyn StateSyncClient>,
    block_info: ConsensusBlockInfo,
    proposal_id: ProposalId,
    timeout_plus_margin: Duration,
    clock: &dyn Clock,
) -> ValidateProposalResult<()> {
    let chrono_timeout = chrono::Duration::from_std(timeout_plus_margin)
        .expect("Can't convert timeout to chrono::Duration");

    let input = ValidateBlockInput {
        proposal_id,
        deadline: clock.now() + chrono_timeout,
        retrospective_block_hash: retrospective_block_hash(state_sync_client, &block_info).await?,
        block_info: convert_to_sn_api_block_info(&block_info)?,
    };
    debug!("Initiating validate proposal: input={input:?}");
    batcher.validate_block(input.clone()).await.map_err(|err| {
        ValidateProposalError::Batcher(
            format!("Failed to initiate validate proposal {input:?}."),
            err,
        )
    })?;
    Ok(())
}

/// Handles receiving a proposal from another node without blocking consensus:
/// 1. Receives the proposal part from the network.
/// 2. Pass this to the batcher.
/// 3. Once finished, receive the commitment from the batcher.
async fn handle_proposal_part(
    proposal_id: ProposalId,
    batcher: &dyn BatcherClient,
    proposal_part: Option<ProposalPart>,
    content: &mut Vec<Vec<InternalConsensusTransaction>>,
    final_n_executed_txs: &mut Option<usize>,
    transaction_converter: Arc<dyn TransactionConverterTrait>,
) -> HandledProposalPart {
    match proposal_part {
        None => HandledProposalPart::Failed("Failed to receive proposal content".to_string()),
        Some(ProposalPart::Fin(fin)) => {
            info!("Received fin={fin:?}");
            let Some(final_n_executed_txs_nonopt) = *final_n_executed_txs else {
                return HandledProposalPart::Failed(
                    "Received Fin without executed transaction count".to_string(),
                );
            };
            // Output this along with the ID from batcher, to compare them.
            let input = SendProposalContentInput {
                proposal_id,
                content: SendProposalContent::Finish(final_n_executed_txs_nonopt),
            };
            let response = batcher.send_proposal_content(input).await.unwrap_or_else(|e| {
                panic!("Failed to send Fin to batcher: {proposal_id:?}. {e:?}")
            });
            let response_id = match response.response {
                ProposalStatus::Finished(id) => id,
                ProposalStatus::InvalidProposal => return HandledProposalPart::Invalid,
                status => panic!("Unexpected status: for {proposal_id:?}, {status:?}"),
            };
            let batcher_block_id = BlockHash(response_id.state_diff_commitment.0.0);

            info!(
                network_block_id = ?fin.proposal_commitment,
                ?batcher_block_id,
                final_n_executed_txs_nonopt,
                "Finished validating proposal."
            );
            if final_n_executed_txs_nonopt == 0 {
                warn!("Validated an empty proposal.");
            }
            HandledProposalPart::Finished(batcher_block_id, fin)
        }
        Some(ProposalPart::Transactions(TransactionBatch { transactions: txs })) => {
            debug!("Received transaction batch with {} txs", txs.len());
            if final_n_executed_txs.is_some() {
                return HandledProposalPart::Failed(
                    "Received transactions after executed transaction count".to_string(),
                );
            }
            let txs =
                futures::future::join_all(txs.into_iter().map(|tx| {
                    transaction_converter.convert_consensus_tx_to_internal_consensus_tx(tx)
                }))
                .await
                .into_iter()
                .collect::<Result<Vec<_>, _>>();
            let txs = match txs {
                Ok(txs) => txs,
                Err(e) => {
                    return HandledProposalPart::Failed(format!(
                        "Failed to convert transactions. Stopping the build of the current \
                         proposal. {e:?}"
                    ));
                }
            };
            debug!(
                "Converted transactions to internal representation. hashes={:?}",
                txs.iter().map(|tx| tx.tx_hash()).collect::<Vec<TransactionHash>>()
            );

            content.push(txs.clone());
            let input =
                SendProposalContentInput { proposal_id, content: SendProposalContent::Txs(txs) };
            let response = batcher.send_proposal_content(input).await.unwrap_or_else(|e| {
                panic!("Failed to send proposal content to batcher: {proposal_id:?}. {e:?}")
            });
            match response.response {
                ProposalStatus::Processing => HandledProposalPart::Continue,
                ProposalStatus::InvalidProposal => HandledProposalPart::Invalid,
                status => panic!("Unexpected status: for {proposal_id:?}, {status:?}"),
            }
        }
        Some(ProposalPart::ExecutedTransactionCount(executed_txs_count)) => {
            debug!("Received executed transaction count: {executed_txs_count}");
            if final_n_executed_txs.is_some() {
                return HandledProposalPart::Failed(
                    "Received executed transaction count more than once".to_string(),
                );
            }
            let executed_txs_count_usize_res: Result<usize, _> = executed_txs_count.try_into();
            let Ok(executed_txs_count_usize) = executed_txs_count_usize_res else {
                return HandledProposalPart::Failed(
                    "Number of executed transactions should fit in usize".to_string(),
                );
            };
            *final_n_executed_txs = Some(executed_txs_count_usize);
            *content = truncate_to_executed_txs(content, executed_txs_count_usize);

            HandledProposalPart::Continue
        }
        _ => HandledProposalPart::Failed("Invalid proposal part".to_string()),
    }
}

async fn batcher_abort_proposal(batcher: &dyn BatcherClient, proposal_id: ProposalId) {
    let input = SendProposalContentInput { proposal_id, content: SendProposalContent::Abort };
    batcher
        .send_proposal_content(input)
        .await
        .unwrap_or_else(|e| panic!("Failed to send Abort to batcher: {proposal_id:?}. {e:?}"));
}
