use std::sync::{Arc, Mutex};
use std::time::Duration;

use apollo_batcher_types::batcher_types::{
    GetProposalContent,
    GetProposalContentInput,
    ProposalId,
    ProposeBlockInput,
};
use apollo_batcher_types::communication::{BatcherClient, BatcherClientError};
use apollo_class_manager_types::transaction_converter::{
    TransactionConverterError,
    TransactionConverterTrait,
};
use apollo_consensus::types::{ProposalCommitment, Round};
use apollo_l1_gas_price_types::errors::{EthToStrkOracleClientError, L1GasPriceClientError};
use apollo_protobuf::consensus::{
    ConsensusBlockInfo,
    ProposalFin,
    ProposalInit,
    ProposalPart,
    TransactionBatch,
};
use apollo_state_sync_types::communication::StateSyncClientError;
use futures::channel::mpsc;
use futures::{FutureExt, SinkExt};
use starknet_api::block::{BlockHash, GasPrice};
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::core::ContractAddress;
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::transaction::TransactionHash;
use tokio_util::sync::CancellationToken;
use tokio_util::task::AbortOnDropHandle;
use tracing::{debug, error, info, trace, warn};

use crate::sequencer_consensus_context::{BuiltProposals, SequencerConsensusContextDeps};
use crate::utils::{
    convert_to_sn_api_block_info,
    get_oracle_rate_and_prices,
    retrospective_block_hash,
    truncate_to_executed_txs,
    GasPriceParams,
};

pub(crate) struct ProposalBuildArguments {
    pub deps: SequencerConsensusContextDeps,
    pub batcher_timeout: Duration,
    pub proposal_init: ProposalInit,
    pub l1_da_mode: L1DataAvailabilityMode,
    pub proposal_sender: mpsc::Sender<ProposalPart>,
    pub gas_price_params: GasPriceParams,
    pub valid_proposals: Arc<Mutex<BuiltProposals>>,
    pub proposal_id: ProposalId,
    pub cende_write_success: AbortOnDropHandle<bool>,
    pub l2_gas_price: GasPrice,
    pub builder_address: ContractAddress,
    pub cancel_token: CancellationToken,
    pub previous_block_info: Option<ConsensusBlockInfo>,
    pub proposal_round: Round,
}

type BuildProposalResult<T> = Result<T, BuildProposalError>;

#[derive(Debug, thiserror::Error)]
pub(crate) enum BuildProposalError {
    #[error("Batcher error: {0}")]
    Batcher(#[from] BatcherClientError),
    #[error("State sync client error: {0}")]
    StateSyncClientError(#[from] StateSyncClientError),
    #[error("State sync is not ready: {0}")]
    StateSyncNotReady(String),
    #[error("EthToStrkOracle error: {0}")]
    EthToStrkOracle(#[from] EthToStrkOracleClientError),
    #[error("L1GasPriceProvider error: {0}")]
    L1GasPriceProvider(#[from] L1GasPriceClientError),
    #[error("Proposal interrupted.")]
    Interrupted,
    #[error(transparent)]
    SendError(#[from] mpsc::SendError),
    #[error("Writing blob to Aerospike failed. {0}")]
    CendeWriteError(String),
    #[error("Failed to convert transactions: {0}")]
    TransactionConverterError(#[from] TransactionConverterError),
}

// Handles building a new proposal without blocking consensus:
pub(crate) async fn build_proposal(
    mut args: ProposalBuildArguments,
) -> BuildProposalResult<ProposalCommitment> {
    let block_info = initiate_build(&args).await?;
    args.proposal_sender
        .send(ProposalPart::Init(args.proposal_init))
        .await
        .expect("Failed to send proposal init");
    args.proposal_sender
        .send(ProposalPart::BlockInfo(block_info.clone()))
        .await
        .expect("Failed to send block info");

    let (proposal_commitment, content) = get_proposal_content(
        args.proposal_id,
        args.deps.batcher.as_ref(),
        args.proposal_sender,
        args.cende_write_success,
        args.deps.transaction_converter,
        args.cancel_token,
    )
    .await?;

    // Update valid_proposals before sending fin to avoid a race condition
    // with `repropose` being called before `valid_proposals` is updated.
    let mut valid_proposals = args.valid_proposals.lock().expect("Lock was poisoned");
    valid_proposals.insert_proposal_for_height(
        &args.proposal_init.height,
        &proposal_commitment,
        block_info,
        content,
        &args.proposal_id,
    );
    Ok(proposal_commitment)
}

async fn initiate_build(args: &ProposalBuildArguments) -> BuildProposalResult<ConsensusBlockInfo> {
    let batcher_timeout = chrono::Duration::from_std(args.batcher_timeout)
        .expect("Can't convert timeout to chrono::Duration");
    let timestamp = args.deps.clock.unix_now();
    let (eth_to_fri_rate, l1_prices) = get_oracle_rate_and_prices(
        args.deps.eth_to_strk_oracle_client.clone(),
        args.deps.l1_gas_price_provider.clone(),
        timestamp,
        args.previous_block_info.as_ref(),
        &args.gas_price_params,
    )
    .await;

    let block_info = ConsensusBlockInfo {
        height: args.proposal_init.height,
        timestamp,
        builder: args.builder_address,
        l1_da_mode: args.l1_da_mode,
        l2_gas_price_fri: args.l2_gas_price,
        l1_gas_price_wei: l1_prices.base_fee_per_gas,
        l1_data_gas_price_wei: l1_prices.blob_fee,
        eth_to_fri_rate,
    };

    let retrospective_block_hash =
        retrospective_block_hash(args.deps.state_sync_client.clone(), &block_info).await?;
    let build_proposal_input = ProposeBlockInput {
        proposal_id: args.proposal_id,
        deadline: args.deps.clock.now() + batcher_timeout,
        retrospective_block_hash,
        block_info: convert_to_sn_api_block_info(&block_info),
        proposal_round: args.proposal_round,
    };
    debug!("Initiating build proposal: {build_proposal_input:?}");
    args.deps.batcher.propose_block(build_proposal_input).await?;
    Ok(block_info)
}
/// 1. Receive chunks of content from the batcher.
/// 2. Forward these to the stream handler to be streamed out to the network.
/// 3. Once finished, receive the commitment from the batcher.
async fn get_proposal_content(
    proposal_id: ProposalId,
    batcher: &dyn BatcherClient,
    mut proposal_sender: mpsc::Sender<ProposalPart>,
    cende_write_success: AbortOnDropHandle<bool>,
    transaction_converter: Arc<dyn TransactionConverterTrait>,
    cancel_token: CancellationToken,
) -> BuildProposalResult<(ProposalCommitment, Vec<Vec<InternalConsensusTransaction>>)> {
    let mut content = Vec::new();
    loop {
        if cancel_token.is_cancelled() {
            return Err(BuildProposalError::Interrupted);
        }
        // We currently want one part of the node failing to cause all components to fail. If this
        // changes, we can simply return None and consider this as a failed proposal which consensus
        // should support.
        let response =
            batcher.get_proposal_content(GetProposalContentInput { proposal_id }).await?;

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
                    transaction_converter.convert_internal_consensus_tx_to_consensus_tx(tx)
                }))
                .await
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?;

                trace!(?transactions, "Sending transaction batch with {} txs.", transactions.len());
                proposal_sender
                    .send(ProposalPart::Transactions(TransactionBatch { transactions }))
                    .await
                    .expect("Failed to broadcast proposal content");
            }
            GetProposalContent::Finished { id, final_n_executed_txs } => {
                let proposal_commitment = BlockHash(id.state_diff_commitment.0.0);
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
                // can't finish the proposal.
                match cende_write_success.now_or_never() {
                    Some(Ok(true)) => {
                        info!("Writing blob to Aerospike completed successfully.");
                    }
                    Some(Ok(false)) => {
                        return Err(BuildProposalError::CendeWriteError("".to_string()));
                    }
                    Some(Err(e)) => {
                        return Err(BuildProposalError::CendeWriteError(e.to_string()));
                    }
                    None => {
                        return Err(BuildProposalError::CendeWriteError(
                            "didn't return in time".to_string(),
                        ));
                    }
                }

                let final_n_executed_txs_u64 = final_n_executed_txs
                    .try_into()
                    .expect("Number of executed transactions should fit in u64");
                proposal_sender
                    .send(ProposalPart::ExecutedTransactionCount(final_n_executed_txs_u64))
                    .await
                    .expect("Failed to broadcast executed transaction count");
                let fin = ProposalFin { proposal_commitment };
                info!("Sending fin={fin:?}");
                proposal_sender
                    .send(ProposalPart::Fin(fin))
                    .await
                    .expect("Failed to broadcast proposal fin");
                return Ok((proposal_commitment, content));
            }
        }
    }
}
