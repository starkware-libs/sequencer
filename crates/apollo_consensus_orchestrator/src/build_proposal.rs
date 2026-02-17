#[cfg(test)]
#[path = "build_proposal_test.rs"]
mod build_proposal_test;

use std::borrow::BorrowMut;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use apollo_batcher_types::batcher_types::{
    GetProposalContent,
    GetProposalContentInput,
    ProposalId,
    ProposeBlockInput,
};
use apollo_batcher_types::communication::BatcherClientError;
use apollo_consensus::types::{ProposalCommitment, Round};
use apollo_l1_gas_price_types::errors::{EthToStrkOracleClientError, L1GasPriceClientError};
use apollo_protobuf::consensus::{
    BuildParam,
    ProposalFin,
    ProposalInit,
    ProposalPart,
    TransactionBatch,
};
use apollo_state_sync_types::communication::SharedStateSyncClient;
use apollo_time::time::{Clock, DateTime};
use apollo_transaction_converter::TransactionConverterError;
use starknet_api::block::GasPrice;
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::core::ContractAddress;
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::transaction::TransactionHash;
use starknet_api::StarknetApiError;
use strum::{EnumDiscriminants, EnumIter, EnumVariantNames, IntoStaticStr};
use tokio_util::sync::CancellationToken;
use tokio_util::task::AbortOnDropHandle;
use tracing::{debug, info, trace, warn};

use crate::sequencer_consensus_context::{BuiltProposals, SequencerConsensusContextDeps};
use crate::utils::{
    convert_to_sn_api_block_info,
    get_l1_prices_in_fri_and_wei,
    truncate_to_executed_txs,
    wait_for_retrospective_block_hash,
    GasPriceParams,
    PreviousBlockInfo,
    RetrospectiveBlockHashError,
    StreamSender,
};

// Minimal wait time that avoids an immediate timeout.
const MIN_WAIT_DURATION: Duration = Duration::from_millis(1);
pub(crate) struct ProposalBuildArguments {
    pub deps: SequencerConsensusContextDeps,
    pub batcher_deadline: DateTime,
    pub build_param: BuildParam,
    pub l1_da_mode: L1DataAvailabilityMode,
    pub stream_sender: StreamSender,
    pub gas_price_params: GasPriceParams,
    pub valid_proposals: Arc<Mutex<BuiltProposals>>,
    pub proposal_id: ProposalId,
    pub cende_write_success: AbortOnDropHandle<bool>,
    pub l2_gas_price: GasPrice,
    pub builder_address: ContractAddress,
    pub cancel_token: CancellationToken,
    pub previous_block_info: Option<PreviousBlockInfo>,
    pub proposal_round: Round,
    pub retrospective_block_hash_deadline: DateTime,
    pub retrospective_block_hash_retry_interval_millis: Duration,
    pub use_state_sync_block_timestamp: bool,
}

type BuildProposalResult<T> = Result<T, BuildProposalError>;

#[derive(Debug, thiserror::Error, EnumDiscriminants)]
#[strum_discriminants(
    name(BuildProposalFailureReasonLabelValue),
    derive(IntoStaticStr, EnumIter, EnumVariantNames),
    strum(serialize_all = "snake_case")
)]
pub(crate) enum BuildProposalError {
    #[error("Batcher error: {0}")]
    Batcher(String, BatcherClientError),
    #[error(transparent)]
    RetrospectiveBlockHashError(#[from] RetrospectiveBlockHashError),
    #[error("Failed to send proposal part: {0}")]
    SendError(String),
    #[error("EthToStrkOracle error: {0}")]
    EthToStrkOracle(#[from] EthToStrkOracleClientError),
    #[error("L1GasPriceProvider error: {0}")]
    L1GasPriceProvider(#[from] L1GasPriceClientError),
    #[error("Proposal interrupted.")]
    Interrupted,
    // TODO(shahak): Add the CENDE_FAILURE warn logs into the error and erase them.
    #[error(
        "Writing blob to Aerospike failed. {0}. For more info search CENDE_FAILURE in the logs"
    )]
    CendeWriteError(String),
    #[error("Failed to convert transactions: {0}")]
    TransactionConverterError(#[from] TransactionConverterError),
    #[error("Block info conversion error: {0}")]
    BlockInfoConversion(#[from] StarknetApiError),
}

// Handles building a new proposal without blocking consensus:
pub(crate) async fn build_proposal(
    mut args: ProposalBuildArguments,
) -> BuildProposalResult<ProposalCommitment> {
    let init = initiate_build(&mut args).await?;
    let height = init.height;

    args.stream_sender
        .send(ProposalPart::Init(init.clone()))
        .await
        .map_err(|e| BuildProposalError::SendError(format!("Failed to send init: {e:?}")))?;

    let (proposal_commitment, content) = get_proposal_content(&mut args).await?;

    // Update valid_proposals before sending fin to avoid a race condition
    // with `repropose` being called before `valid_proposals` is updated.
    let mut valid_proposals = args.valid_proposals.lock().expect("Lock was poisoned");
    valid_proposals.insert_proposal_for_height(
        &height,
        &proposal_commitment,
        init,
        content,
        &args.proposal_id,
    );
    Ok(proposal_commitment)
}

async fn get_proposal_timestamp(
    use_state_sync_block_timestamp: bool,
    state_sync_client: &SharedStateSyncClient,
    clock: &dyn Clock,
) -> u64 {
    if use_state_sync_block_timestamp {
        if let Ok(Some(block_header)) = state_sync_client.get_latest_block_header().await {
            return block_header.block_header_without_hash.timestamp.0;
        }
        warn!("No latest block header available from state sync, falling back to clock time");
    }
    clock.unix_now()
}

async fn initiate_build(args: &mut ProposalBuildArguments) -> BuildProposalResult<ProposalInit> {
    let timestamp = get_proposal_timestamp(
        args.use_state_sync_block_timestamp,
        &args.deps.state_sync_client,
        args.deps.clock.as_ref(),
    )
    .await;
    let (l1_prices_fri, l1_prices_wei) = get_l1_prices_in_fri_and_wei(
        args.deps.l1_gas_price_provider.clone(),
        timestamp,
        args.previous_block_info.as_ref(),
        &args.gas_price_params,
    )
    .await;
    let init = ProposalInit {
        height: args.build_param.height,
        round: args.build_param.round,
        valid_round: args.build_param.valid_round,
        proposer: args.build_param.proposer,
        builder: args.builder_address,
        timestamp,
        l1_da_mode: args.l1_da_mode,
        l2_gas_price_fri: args.l2_gas_price,
        l1_gas_price_wei: l1_prices_wei.l1_gas_price,
        l1_data_gas_price_wei: l1_prices_wei.l1_data_gas_price,
        l1_gas_price_fri: l1_prices_fri.l1_gas_price,
        l1_data_gas_price_fri: l1_prices_fri.l1_data_gas_price,
        starknet_version: starknet_api::block::StarknetVersion::LATEST,
        // TODO(Asmaa): Put the real value once we have it.
        version_constant_commitment: Default::default(),
    };

    let retrospective_block_hash = wait_for_retrospective_block_hash(
        args.deps.batcher.clone(),
        args.deps.state_sync_client.clone(),
        &init,
        args.deps.clock.as_ref(),
        args.retrospective_block_hash_deadline,
        args.retrospective_block_hash_retry_interval_millis,
    )
    .await?;

    let build_proposal_input = ProposeBlockInput {
        proposal_id: args.proposal_id,
        deadline: args.batcher_deadline,
        retrospective_block_hash,
        block_info: convert_to_sn_api_block_info(&init)?,
        proposal_round: args.proposal_round,
    };
    debug!("Initiating build proposal: {build_proposal_input:?}");
    args.deps.batcher.propose_block(build_proposal_input.clone()).await.map_err(|err| {
        BuildProposalError::Batcher(
            format!("Failed to initiate build proposal {build_proposal_input:?}."),
            err,
        )
    })?;
    Ok(init)
}
/// 1. Receive chunks of content from the batcher.
/// 2. Forward these to the stream handler to be streamed out to the network.
/// 3. Once finished, receive the commitment from the batcher.
async fn get_proposal_content(
    args: &mut ProposalBuildArguments,
) -> BuildProposalResult<(ProposalCommitment, Vec<Vec<InternalConsensusTransaction>>)> {
    let mut content = Vec::new();
    loop {
        if args.cancel_token.is_cancelled() {
            return Err(BuildProposalError::Interrupted);
        }
        // We currently want one part of the node failing to cause all components to fail. If this
        // changes, we can simply return None and consider this as a failed proposal which consensus
        // should support.
        let response = args
            .deps
            .batcher
            .get_proposal_content(GetProposalContentInput { proposal_id: args.proposal_id })
            .await
            .map_err(|err| {
                BuildProposalError::Batcher(
                    format!("Failed to get proposal content for proposal_id {}.", args.proposal_id),
                    err,
                )
            })?;

        match response.content {
            GetProposalContent::Txs(txs) => {
                content.push(txs.clone());
                // TODO(matan): Make sure this isn't too large for a single proto message.
                debug!(
                    hashes = ?txs.iter().map(|tx| tx.tx_hash()).collect::<Vec<TransactionHash>>(),
                    "Sending transaction batch with {} txs.",
                    txs.len()
                );
                let transactions = futures::future::join_all(txs.into_iter().map(|tx| {
                    args.deps
                        .transaction_converter
                        .convert_internal_consensus_tx_to_consensus_tx(tx)
                }))
                .await
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?;

                trace!(?transactions, "Sending transaction batch with {} txs.", transactions.len());
                args.stream_sender
                    .send(ProposalPart::Transactions(TransactionBatch { transactions }))
                    .await
                    .map_err(|e| {
                        BuildProposalError::SendError(format!(
                            "Failed to send transaction batch: {e:?}"
                        ))
                    })?;
            }
            GetProposalContent::Finished { id, final_n_executed_txs } => {
                let proposal_commitment = ProposalCommitment(id.state_diff_commitment.0.0);
                content = truncate_to_executed_txs(&mut content, final_n_executed_txs);

                info!(
                    ?proposal_commitment,
                    num_txs = final_n_executed_txs,
                    "Finished building proposal",
                );
                if final_n_executed_txs == 0 {
                    warn!("Built an empty proposal.");
                }

                // If the blob writing operation to Aerospike doesn't return a success status, we
                // can't finish the proposal. Must wait for it at least until batcher_timeout is
                // reached.
                let remaining_duration = (args.batcher_deadline - args.deps.clock.now())
                    .to_std()
                    .unwrap_or_default()
                    .max(MIN_WAIT_DURATION); // Ensure we wait at least 1 ms to avoid immediate timeout.
                match tokio::time::timeout(
                    remaining_duration,
                    args.cende_write_success.borrow_mut(),
                )
                .await
                {
                    Err(_) => {
                        return Err(BuildProposalError::CendeWriteError(
                            "Writing blob to Aerospike didn't return in time.".to_string(),
                        ));
                    }
                    Ok(Ok(true)) => {
                        info!("Writing blob to Aerospike completed successfully.");
                    }
                    Ok(Ok(false)) => {
                        return Err(BuildProposalError::CendeWriteError(
                            "Writing blob to Aerospike failed.".to_string(),
                        ));
                    }
                    Ok(Err(e)) => {
                        return Err(BuildProposalError::CendeWriteError(e.to_string()));
                    }
                }

                let executed_transaction_count: u64 = final_n_executed_txs
                    .try_into()
                    .expect("Number of executed transactions should fit in u64");
                let fin = ProposalFin {
                    proposal_commitment,
                    executed_transaction_count,
                    commitment_parts: None,
                };
                info!("Sending fin={fin:?}");
                args.stream_sender.send(ProposalPart::Fin(fin)).await.map_err(|e| {
                    BuildProposalError::SendError(format!("Failed to send proposal fin: {e:?}"))
                })?;
                return Ok((proposal_commitment, content));
            }
        }
    }
}
