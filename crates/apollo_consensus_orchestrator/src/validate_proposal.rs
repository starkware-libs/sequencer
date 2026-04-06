#[cfg(test)]
#[path = "validate_proposal_test.rs"]
mod validate_proposal_test;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use apollo_batcher_types::batcher_types::{
    FinishProposalInput,
    FinishProposalStatus,
    FinishedProposalInfo,
    ProposalId,
    SendTxsForProposalInput,
    SendTxsForProposalStatus,
    ValidateBlockInput,
};
use apollo_batcher_types::communication::{BatcherClient, BatcherClientError};
use apollo_batcher_types::errors::BatcherError;
use apollo_consensus::types::ProposalCommitment;
use apollo_l1_gas_price_types::errors::{EthToStrkOracleClientError, L1GasPriceClientError};
use apollo_l1_gas_price_types::L1GasPriceProviderClient;
use apollo_protobuf::consensus::{ProposalFin, ProposalInit, ProposalPart, TransactionBatch};
use apollo_state_sync_types::communication::SharedStateSyncClient;
use apollo_time::time::{Clock, ClockExt, DateTime};
use apollo_transaction_converter::{TransactionConverterTrait, VerifyAndStoreProofTask};
use futures::channel::mpsc;
use futures::StreamExt;
use starknet_api::block::{BlockNumber, GasPrice};
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::core::ContractAddress;
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::transaction::TransactionHash;
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;
use starknet_api::StarknetApiError;
use strum::{EnumDiscriminants, EnumIter, IntoStaticStr, VariantNames};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, instrument, warn};

use crate::metrics::{
    CONSENSUS_NUM_BATCHES_IN_PROPOSAL,
    CONSENSUS_NUM_TXS_IN_PROPOSAL,
    CONSENSUS_PROPOSAL_FIN_MISMATCH,
};
use crate::orchestrator_versioned_constants::VersionedConstants;
use crate::sequencer_consensus_context::{BuiltProposals, SequencerConsensusContextDeps};
use crate::utils::{
    convert_to_sn_api_block_info,
    get_l1_prices_in_fri_and_wei,
    retrospective_block_hash,
    truncate_to_executed_txs,
    GasPriceParams,
    PreviousProposalInitInfo,
    RetrospectiveBlockHashError,
};

const GAS_PRICE_ABS_DIFF_MARGIN: u128 = 1;

pub(crate) struct ProposalValidateArguments {
    pub deps: SequencerConsensusContextDeps,
    pub init: ProposalInit,
    pub proposal_init_validation: ProposalInitValidation,
    pub proposal_id: ProposalId,
    pub timeout: Duration,
    pub batcher_timeout_margin: Duration,
    pub valid_proposals: Arc<Mutex<BuiltProposals>>,
    pub content_receiver: mpsc::Receiver<ProposalPart>,
    pub gas_price_params: GasPriceParams,
    pub cancel_token: CancellationToken,
    pub compare_retrospective_block_hash: bool,
}

// Contains parameters required for validating ProposalInit.
#[derive(Clone, Debug)]
pub(crate) struct ProposalInitValidation {
    pub height: BlockNumber,
    pub builder_address: ContractAddress,
    pub block_timestamp_window_seconds: u64,
    pub previous_proposal_init: Option<PreviousProposalInitInfo>,
    pub l1_da_mode: L1DataAvailabilityMode,
    pub l2_gas_price_fri: GasPrice,
}

/// Parameters for deadline and cancellation handling during proposal finalization.
struct ProposalDeadlineParams {
    clock: Arc<dyn Clock>,
    deadline: DateTime,
    cancel_token: CancellationToken,
}

enum HandledProposalPart {
    Continue,
    Invalid(String),
    Finished(ProposalCommitment, Box<ProposalFin>, FinishedProposalInfo),
    Failed(String),
    Timeout(String),
    Interrupted(String),
}

type ValidateProposalResult<T> = Result<T, ValidateProposalError>;

#[derive(Debug, thiserror::Error, EnumDiscriminants)]
#[strum_discriminants(
    name(ValidateProposalFailureReasonLabelValue),
    derive(IntoStaticStr, EnumIter, VariantNames),
    strum(serialize_all = "snake_case")
)]
pub(crate) enum ValidateProposalError {
    #[error("Batcher error: {0}")]
    Batcher(String, BatcherClientError),
    #[error(transparent)]
    RetrospectiveBlockHashError(#[from] RetrospectiveBlockHashError),
    // Consensus may exit early (e.g. sync).
    #[error("Failed to send commitment to consensus: {0}")]
    SendError(ProposalCommitment),
    #[error("EthToStrkOracle error: {0}")]
    EthToStrkOracle(#[from] EthToStrkOracleClientError),
    #[error("L1GasPriceProvider error: {0}")]
    L1GasPriceProvider(#[from] L1GasPriceClientError),
    #[error("ProposalInit conversion error: {0}")]
    ProposalInitConversion(#[from] StarknetApiError),
    #[error("Invalid ProposalInit: {2}. received:{0:?}, validation criteria {1:?}.")]
    InvalidProposalInit(ProposalInit, ProposalInitValidation, String),
    #[error("Validation timed out while {0}")]
    ValidationTimeout(String),
    #[error("Proposal interrupted while {0}")]
    ProposalInterrupted(String),
    #[error("Batcher returned Invalid status: {0}.")]
    InvalidProposal(String),
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
    let mut verify_and_store_proof_tasks: Vec<VerifyAndStoreProofTask> = Vec::new();
    let now = args.deps.clock.now();

    let Some(deadline) = now.checked_add_signed(chrono::TimeDelta::from_std(args.timeout).unwrap())
    else {
        return Err(ValidateProposalError::CannotCalculateDeadline { timeout: args.timeout, now });
    };

    is_proposal_init_valid(
        &args.proposal_init_validation,
        &args.init,
        args.deps.clock.as_ref(),
        args.deps.l1_gas_price_provider,
        &args.gas_price_params,
    )
    .await?;

    initiate_validation(
        args.deps.batcher.clone(),
        args.deps.state_sync_client,
        &args.init,
        args.proposal_id,
        args.timeout + args.batcher_timeout_margin,
        args.deps.clock.as_ref(),
        args.compare_retrospective_block_hash,
    )
    .await?;

    let deadline_params = ProposalDeadlineParams {
        clock: args.deps.clock.clone(),
        deadline,
        cancel_token: args.cancel_token.clone(),
    };

    // Validating the rest of the proposal parts.
    let (built_block, received_fin, finished_info) = loop {
        tokio::select! {
            _ = args.cancel_token.cancelled() => {
                // Ignoring batcher errors, to better reflect the proposal interruption.
                batcher_abort_proposal(args.deps.batcher.as_ref(), args.proposal_id).await.ok();
                return Err(ValidateProposalError::ProposalInterrupted(
                    "validating proposal parts".to_string(),
                ));
            }
            _ = args.deps.clock.sleep_until(deadline) => {
                // Ignoring batcher errors, to better reflect the proposal deadline timeout.
                batcher_abort_proposal(args.deps.batcher.as_ref(), args.proposal_id).await.ok();
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
                    &mut verify_and_store_proof_tasks,
                    args.deps.transaction_converter.clone(),
                    &deadline_params,
                ).await {
                    HandledProposalPart::Finished(built_block, received_fin, finished_info) => {
                        break (built_block, received_fin, finished_info);
                    }
                    HandledProposalPart::Continue => {continue;}
                    HandledProposalPart::Invalid(err) => {
                        // No need to abort since the Batcher is the source of this info.
                        return Err(ValidateProposalError::InvalidProposal(err));
                    }
                    HandledProposalPart::Failed(fail_reason) => {
                        batcher_abort_proposal(args.deps.batcher.as_ref(), args.proposal_id).await?;
                        return Err(ValidateProposalError::ProposalPartFailed(fail_reason,proposal_part));
                    }
                    HandledProposalPart::Timeout(msg) => {
                        // Ignoring batcher errors, to better reflect the validation timeout.
                        batcher_abort_proposal(args.deps.batcher.as_ref(), args.proposal_id).await.ok();
                        return Err(ValidateProposalError::ValidationTimeout(msg));
                    }
                    HandledProposalPart::Interrupted(msg) => {
                        // Ignoring batcher errors, to better reflect the proposal interruption.
                        batcher_abort_proposal(args.deps.batcher.as_ref(), args.proposal_id).await.ok();
                        return Err(ValidateProposalError::ProposalInterrupted(msg));
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
    valid_proposals.insert_proposal(args.init, content, &args.proposal_id, finished_info);

    // TODO(matan): Switch to signature validation.
    if built_block != received_fin.proposal_commitment {
        CONSENSUS_PROPOSAL_FIN_MISMATCH.increment(1);
        return Err(ValidateProposalError::ProposalFinMismatch);
    }

    Ok(built_block)
}

#[instrument(level = "warn", skip_all, fields(?proposal_init_validation, ?init_proposed))]
async fn is_proposal_init_valid(
    proposal_init_validation: &ProposalInitValidation,
    init_proposed: &ProposalInit,
    clock: &dyn Clock,
    l1_gas_price_provider: Arc<dyn L1GasPriceProviderClient>,
    gas_price_params: &GasPriceParams,
) -> ValidateProposalResult<()> {
    let now: u64 = clock.unix_now();
    let last_block_timestamp =
        proposal_init_validation.previous_proposal_init.as_ref().map_or(0, |info| info.timestamp);
    if init_proposed.timestamp < last_block_timestamp {
        return Err(ValidateProposalError::InvalidProposalInit(
            init_proposed.clone(),
            proposal_init_validation.clone(),
            format!(
                "Timestamp is too old: last_block_timestamp={}, proposed={}",
                last_block_timestamp, init_proposed.timestamp
            ),
        ));
    }
    if init_proposed.timestamp > now + proposal_init_validation.block_timestamp_window_seconds {
        return Err(ValidateProposalError::InvalidProposalInit(
            init_proposed.clone(),
            proposal_init_validation.clone(),
            format!(
                "Timestamp is in the future: now={}, block_timestamp_window_seconds={}, \
                 proposed={}",
                now,
                proposal_init_validation.block_timestamp_window_seconds,
                init_proposed.timestamp
            ),
        ));
    }
    if !(init_proposed.height == proposal_init_validation.height
        && init_proposed.l1_da_mode == proposal_init_validation.l1_da_mode
        && init_proposed.l2_gas_price_fri == proposal_init_validation.l2_gas_price_fri)
    {
        return Err(ValidateProposalError::InvalidProposalInit(
            init_proposed.clone(),
            proposal_init_validation.clone(),
            "ProposalInit validation failed".to_string(),
        ));
    }
    if init_proposed.builder != proposal_init_validation.builder_address {
        return Err(ValidateProposalError::InvalidProposalInit(
            init_proposed.clone(),
            proposal_init_validation.clone(),
            format!(
                "Builder address mismatch: expected={}, proposed={}",
                proposal_init_validation.builder_address, init_proposed.builder
            ),
        ));
    }
    let (l1_gas_prices_fri, _l1_gas_prices_wei) = get_l1_prices_in_fri_and_wei(
        l1_gas_price_provider,
        init_proposed.timestamp,
        proposal_init_validation.previous_proposal_init.as_ref(),
        gas_price_params,
    )
    .await;
    let l1_gas_price_margin_percent =
        VersionedConstants::latest_constants().l1_gas_price_margin_percent.into();
    debug!("L1 price info: {l1_gas_prices_fri:?}");

    let l1_gas_price_fri = l1_gas_prices_fri.l1_gas_price;
    let l1_data_gas_price_fri = l1_gas_prices_fri.l1_data_gas_price;
    let l1_gas_price_fri_proposed = init_proposed.l1_gas_price_fri;
    let l1_data_gas_price_fri_proposed = init_proposed.l1_data_gas_price_fri;

    if !(within_margin(l1_gas_price_fri_proposed, l1_gas_price_fri, l1_gas_price_margin_percent)
        && within_margin(
            l1_data_gas_price_fri_proposed,
            l1_data_gas_price_fri,
            l1_gas_price_margin_percent,
        ))
    {
        return Err(ValidateProposalError::InvalidProposalInit(
            init_proposed.clone(),
            proposal_init_validation.clone(),
            format!(
                "L1 gas price mismatch: expected L1 gas price FRI={l1_gas_price_fri}, \
                 proposed={l1_gas_price_fri_proposed}, expected L1 data gas price \
                 FRI={l1_data_gas_price_fri}, proposed={l1_data_gas_price_fri_proposed}, \
                 l1_gas_price_margin_percent={l1_gas_price_margin_percent}"
            ),
        ));
    }
    Ok(())
}

fn within_margin(number1: GasPrice, number2: GasPrice, margin_percent: u128) -> bool {
    // For small numbers (e.g., less than 10 wei, if margin is 10%), even an off-by-one
    // error might be bigger than the margin, even if it is just a rounding error.
    // We make an exception for such mismatch, and don't bother checking percentages
    // if the difference in price is only one wei.
    if number1.0.abs_diff(number2.0) <= GAS_PRICE_ABS_DIFF_MARGIN {
        return true;
    }
    let margin = (number1.0 * margin_percent) / 100;
    number1.0.abs_diff(number2.0) <= margin
}

// The second proposal part when validating a proposal must be:
// 1. Fin - empty proposal.
// 2. ProposalInit (init) - required to begin executing TX batches.
async fn initiate_validation(
    batcher: Arc<dyn BatcherClient>,
    state_sync_client: SharedStateSyncClient,
    init: &ProposalInit,
    proposal_id: ProposalId,
    timeout_plus_margin: Duration,
    clock: &dyn Clock,
    compare_retrospective_block_hash: bool,
) -> ValidateProposalResult<()> {
    let chrono_timeout = chrono::Duration::from_std(timeout_plus_margin)
        .expect("Can't convert timeout to chrono::Duration");

    let input = ValidateBlockInput {
        proposal_id,
        deadline: clock.now() + chrono_timeout,
        retrospective_block_hash: retrospective_block_hash(
            batcher.clone(),
            state_sync_client,
            init,
            compare_retrospective_block_hash,
        )
        .await
        .map_err(ValidateProposalError::from)?,
        block_info: convert_to_sn_api_block_info(init)?,
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

/// Awaits a task that verifies and stores a proof spawned during consensus transaction conversion.
async fn await_verify_and_store_proof_task(task: VerifyAndStoreProofTask) -> Result<(), String> {
    task.await
        .map_err(|e| format!("Verify and store proof task panicked: {e}"))?
        .map_err(|e| e.to_string())
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
    verify_and_store_proof_tasks: &mut Vec<VerifyAndStoreProofTask>,
    transaction_converter: Arc<dyn TransactionConverterTrait>,
    deadline_params: &ProposalDeadlineParams,
) -> HandledProposalPart {
    match proposal_part {
        None => {
            // Can happen due to:
            // 1. The StreamHandler evicted this stream.
            // 2. The stream was closed by the Proposer without sending ProposalFin.
            //    - Can occur if the Proposer can't complete the proposal (e.g. error during
            //      build_proposal).
            HandledProposalPart::Failed(
                "Proposal content stream was closed before receiving fin".to_string(),
            )
        }
        Some(ProposalPart::Fin(fin)) => {
            info!("Received fin={fin:?}");
            let Ok(executed_txs_count) = fin.executed_transaction_count.try_into() else {
                return HandledProposalPart::Failed(
                    "Number of executed transactions should fit in usize".to_string(),
                );
            };

            *content = truncate_to_executed_txs(content, executed_txs_count);

            // Verification and store proof tasks are spawned during consensus transaction
            // conversion, running concurrently with batcher execution. Since the fin
            // is always the last proposal part, we await all tasks here to ensure every
            // proof is verified and stored before finalizing the proposal.
            // The tasks themselves are not cancelled, they continue running in the background,
            // but we stop waiting for them.
            let start = Instant::now();
            for task in verify_and_store_proof_tasks.drain(..) {
                tokio::select! {
                    _ = deadline_params.cancel_token.cancelled() => {
                        let duration = start.elapsed();
                        info!("Verification tasks interrupted after {duration:?}");
                        return HandledProposalPart::Interrupted(
                            "awaiting verification tasks".to_string(),
                        );
                    }
                    _ = deadline_params.clock.sleep_until(deadline_params.deadline) => {
                        let duration = start.elapsed();
                        info!("Verification tasks timed out after {duration:?}");
                        return HandledProposalPart::Timeout(
                            "awaiting verification tasks".to_string(),
                        );
                    }
                    result = await_verify_and_store_proof_task(task) => {
                        if let Err(e) = result {
                            return HandledProposalPart::Failed(e);
                        }
                    }
                }
            }
            let duration = start.elapsed();
            info!("Awaiting the verify and store proof tasks took: {duration:?}");

            // Output this along with the ID from batcher, to compare them.
            let input =
                FinishProposalInput { proposal_id, final_n_executed_txs: executed_txs_count };
            let response = match batcher.finish_proposal(input).await {
                Ok(response) => response,
                Err(e) => {
                    return HandledProposalPart::Failed(format!(
                        "Failed to send Fin to batcher: {e:?}"
                    ));
                }
            };
            let finished_info = match response {
                FinishProposalStatus::Finished(info) => info,
                FinishProposalStatus::InvalidProposal(err) => {
                    return HandledProposalPart::Invalid(err);
                }
            };
            let batcher_block_commitment =
                ProposalCommitment(finished_info.proposal_commitment.partial_block_hash.0);

            info!(
                network_block_commitment = ?fin.proposal_commitment,
                ?batcher_block_commitment,
                executed_txs_count,
                "Finished validating proposal."
            );
            if executed_txs_count == 0 {
                warn!("Validated an empty proposal.");
            }
            HandledProposalPart::Finished(batcher_block_commitment, Box::new(fin), finished_info)
        }
        Some(ProposalPart::Transactions(TransactionBatch { transactions: txs })) => {
            // TODO(guyn): check that the length of txs and the number of batches we receive is not
            // so big it would fill up the memory (in case of a malicious proposal)
            debug!("Received transaction batch with {} txs", txs.len());
            let conversion_results =
                futures::future::join_all(txs.into_iter().map(|tx| {
                    transaction_converter.convert_consensus_tx_to_internal_consensus_tx(tx)
                }))
                .await
                .into_iter()
                .collect::<Result<Vec<_>, _>>();
            let conversion_results = match conversion_results {
                Ok(results) => results,
                Err(e) => {
                    return HandledProposalPart::Failed(format!(
                        "Failed to convert transactions. Stopping the build of the current \
                         proposal. {e:?}"
                    ));
                }
            };

            // Separate internal transactions from verification and store proof tasks. Each task
            // verifies the proof and stores it in the proof manager. Tasks are collected
            // and awaited later in the fin case.
            let (txs, tasks): (
                Vec<InternalConsensusTransaction>,
                Vec<Option<VerifyAndStoreProofTask>>,
            ) = conversion_results.into_iter().unzip();
            verify_and_store_proof_tasks.extend(tasks.into_iter().flatten());

            debug!(
                "Converted transactions to internal representation. hashes={:?}",
                txs.iter().map(|tx| tx.tx_hash()).collect::<Vec<TransactionHash>>()
            );

            content.push(txs.clone());
            let input = SendTxsForProposalInput { proposal_id, txs };
            let response = match batcher.send_txs_for_proposal(input).await {
                Ok(response) => response,
                Err(e) => {
                    return HandledProposalPart::Failed(format!(
                        "Failed to send transactions to batcher: {e:?}"
                    ));
                }
            };
            match response {
                SendTxsForProposalStatus::Processing => HandledProposalPart::Continue,
                SendTxsForProposalStatus::InvalidProposal(err) => HandledProposalPart::Invalid(err),
            }
        }
        _ => HandledProposalPart::Failed(format!(
            "Invalid proposal part: {:?}",
            proposal_part.clone()
        )),
    }
}

async fn batcher_abort_proposal(
    batcher: &dyn BatcherClient,
    proposal_id: ProposalId,
) -> Result<(), ValidateProposalError> {
    match batcher.abort_proposal(proposal_id).await {
        Ok(()) => Ok(()),
        Err(BatcherClientError::BatcherError(BatcherError::ProposalAborted)) => {
            warn!("Proposal {proposal_id:?} was already aborted by batcher");
            Ok(())
        }
        Err(e) => {
            warn!("Batcher failed to abort proposal {proposal_id:?}: {e:?}");
            Err(ValidateProposalError::Batcher(
                format!("Failed to abort proposal {proposal_id:?}."),
                e,
            ))
        }
    }
}
