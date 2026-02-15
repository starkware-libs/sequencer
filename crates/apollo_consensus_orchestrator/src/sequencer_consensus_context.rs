//! Implementation of the ConsensusContext interface for running the sequencer.
//!
//! It connects to the Batcher who is responsible for building/validating blocks.
#[cfg(test)]
#[path = "sequencer_consensus_context_test.rs"]
mod sequencer_consensus_context_test;

use std::cmp::max;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use apollo_batcher_types::batcher_types::{
    DecisionReachedInput,
    DecisionReachedResponse,
    ProposalId,
    StartHeightInput,
};
use apollo_batcher_types::communication::BatcherClient;
use apollo_class_manager_types::transaction_converter::{
    TransactionConverterError,
    TransactionConverterTrait,
};
use apollo_config_manager_types::communication::SharedConfigManagerClient;
use apollo_consensus::types::{ConsensusContext, ConsensusError, ProposalCommitment, Round};
use apollo_consensus_orchestrator_config::config::ContextConfig;
use apollo_l1_gas_price_types::L1GasPriceProviderClient;
use apollo_network::network_manager::{BroadcastTopicClient, BroadcastTopicClientTrait};
use apollo_protobuf::consensus::{
    BuildParam,
    HeightAndRound,
    ProposalFin,
    ProposalInit,
    SignedProposalPart,
    TransactionBatch,
    Vote,
};
use apollo_state_sync_types::communication::{StateSyncClient, StateSyncClientError};
use apollo_state_sync_types::errors::StateSyncError;
use apollo_state_sync_types::state_sync_types::SyncBlock;
use apollo_time::time::Clock;
use async_trait::async_trait;
use futures::channel::mpsc::SendError;
use futures::channel::{mpsc, oneshot};
use futures::future::ready;
use futures::SinkExt;
use starknet_api::block::{
    BlockHeaderWithoutHash,
    BlockInfo,
    BlockNumber,
    BlockTimestamp,
    GasPrice,
};
use starknet_api::block_hash::block_hash_calculator::BlockHeaderCommitments;
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::core::SequencerContractAddress;
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::execution_resources::GasAmount;
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::TransactionHash;
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio_util::task::AbortOnDropHandle;
use tracing::{error, error_span, info, instrument, trace, warn, Instrument};

use crate::build_proposal::{build_proposal, BuildProposalError, ProposalBuildArguments};
use crate::cende::{BlobParameters, CendeContext, InternalTransactionWithReceipt};
use crate::fee_market::{
    calculate_next_base_gas_price,
    get_min_gas_price_for_height,
    FeeMarketInfo,
};
use crate::metrics::{
    record_build_proposal_failure,
    record_validate_proposal_failure,
    register_metrics,
    CONSENSUS_L2_GAS_PRICE,
};
use crate::orchestrator_versioned_constants::VersionedConstants;
use crate::utils::{
    convert_to_sn_api_block_info,
    make_gas_price_params,
    L1PricesInFri,
    L1PricesInWei,
    PreviousBlockInfo,
    StreamSender,
};
use crate::validate_proposal::{
    validate_proposal,
    BlockInfoValidation,
    ProposalValidateArguments,
    ValidateProposalError,
};

type ValidationParams = (ProposalInit, Duration, mpsc::Receiver<SignedProposalPart>);

type HeightToIdToContent = BTreeMap<
    BlockNumber,
    BTreeMap<
        ProposalCommitment,
        (ProposalInit, Vec<Vec<InternalConsensusTransaction>>, ProposalId),
    >,
>;

pub(crate) struct BuiltProposals {
    // {height: {proposal_commitment: (init, content, [proposal_ids])}}
    // Note that multiple proposals IDs can be associated with the same content, but we only need
    // to store one of them.
    //
    // The tranasactions are stored as a vector of batches (as returned from the batcher) and not
    // flattened. This is since we might need to repropose, in which case we need to send the
    // transactions in batches.
    data: HeightToIdToContent,
}

impl BuiltProposals {
    pub fn new() -> Self {
        Self { data: HeightToIdToContent::default() }
    }

    fn get_proposal(
        &self,
        height: &BlockNumber,
        commitment: &ProposalCommitment,
    ) -> &(ProposalInit, Vec<Vec<InternalConsensusTransaction>>, ProposalId) {
        self.data
            .get(height)
            .unwrap_or_else(|| panic!("No proposals found for height {height}"))
            .get(commitment)
            .unwrap_or_else(|| panic!("No proposal found for height {height} and id {commitment}"))
    }

    fn remove_proposals_below_or_at_height(&mut self, height: &BlockNumber) {
        self.data.retain(|&h, _| h > *height);
    }

    pub(crate) fn insert_proposal_for_height(
        &mut self,
        height: &BlockNumber,
        proposal_commitment: &ProposalCommitment,
        init: ProposalInit,
        transactions: Vec<Vec<InternalConsensusTransaction>>,
        proposal_id: &ProposalId,
    ) {
        self.data
            .entry(*height)
            .or_default()
            .insert(*proposal_commitment, (init, transactions, *proposal_id));
    }
}

pub struct SequencerConsensusContext {
    config: ContextConfig,
    deps: SequencerConsensusContextDeps,
    // Proposal building/validating returns immediately, leaving the actual processing to a spawned
    // task. The spawned task processes the proposal asynchronously and updates the
    // valid_proposals map upon completion, ensuring consistency across tasks.
    valid_proposals: Arc<Mutex<BuiltProposals>>,
    // Used to generate unique proposal IDs across the lifetime of the context.
    // TODO(matan): Consider robustness in case consensus can restart without the Batcher
    // restarting.
    proposal_id: u64,
    current_height: Option<BlockNumber>,
    current_round: Round,
    // The active proposal refers to the proposal being validated at the current height/round.
    // Building proposals are not tracked as active, as consensus can't move on to the next
    // height/round until building is done. Context only works on proposals for the
    // current round.
    active_proposal: Option<(CancellationToken, JoinHandle<()>)>,
    // Stores proposals for future rounds until the round is reached.
    queued_proposals: BTreeMap<Round, (ValidationParams, oneshot::Sender<ProposalCommitment>)>,
    l2_gas_price: GasPrice,
    l1_da_mode: L1DataAvailabilityMode,
    previous_block_info: Option<PreviousBlockInfo>,
}

#[derive(Clone)]
pub struct SequencerConsensusContextDeps {
    pub transaction_converter: Arc<dyn TransactionConverterTrait>,
    pub state_sync_client: Arc<dyn StateSyncClient>,
    pub batcher: Arc<dyn BatcherClient>,
    pub cende_ambassador: Arc<dyn CendeContext>,
    pub l1_gas_price_provider: Arc<dyn L1GasPriceProviderClient>,
    /// Use DefaultClock if you don't want to inject timestamps.
    pub clock: Arc<dyn Clock>,
    // Used to initiate new outbound proposal streams.
    pub outbound_proposal_sender:
        mpsc::Sender<(HeightAndRound, mpsc::Receiver<SignedProposalPart>)>,
    // Used to broadcast votes to other consensus nodes.
    pub vote_broadcast_client: BroadcastTopicClient<Vote>,
    pub config_manager_client: Option<SharedConfigManagerClient>,
}

#[derive(thiserror::Error, PartialEq, Debug)]
enum ReproposeError {
    #[error(transparent)]
    SendError(#[from] SendError),
    #[error(transparent)]
    ConvertError(#[from] TransactionConverterError),
}

impl SequencerConsensusContext {
    pub fn new(config: ContextConfig, deps: SequencerConsensusContextDeps) -> Self {
        register_metrics();
        let l1_da_mode = if config.static_config.l1_da_mode {
            L1DataAvailabilityMode::Blob
        } else {
            L1DataAvailabilityMode::Calldata
        };
        Self {
            config,
            deps,
            valid_proposals: Arc::new(Mutex::new(BuiltProposals::new())),
            proposal_id: 0,
            current_height: None,
            current_round: 0,
            active_proposal: None,
            queued_proposals: BTreeMap::new(),
            l2_gas_price: VersionedConstants::latest_constants().min_gas_price,
            l1_da_mode,
            previous_block_info: None,
        }
    }

    async fn start_stream(&mut self, stream_id: HeightAndRound) -> StreamSender {
        let (proposal_sender, proposal_receiver) =
            mpsc::channel(self.config.static_config.proposal_buffer_size);
        self.deps
            .outbound_proposal_sender
            .send((stream_id, proposal_receiver))
            .await
            .expect("Failed to send proposal receiver. Receiver channel closed.");
        StreamSender { proposal_sender }
    }

    async fn get_latest_sync_height(&self) -> Option<BlockNumber> {
        match self.deps.state_sync_client.get_latest_block_number().await {
            Ok(height) => height,
            Err(e) => {
                error!("Failed to get latest sync height: {e:?}");
                None
            }
        }
    }

    async fn can_skip_write_prev_height_blob(&self, height: BlockNumber) -> bool {
        if height == BlockNumber(0) {
            return true;
        }
        match self.get_latest_sync_height().await {
            Some(latest_sync_height) => {
                latest_sync_height
                    >= height.prev().expect("Height should be greater than 0. Checked above.")
            }
            None => false,
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn update_state_sync_with_new_block(
        &self,
        height: BlockNumber,
        state_diff: &ThinStateDiff,
        transactions: &[InternalTransactionWithReceipt],
        init: &ProposalInit,
        cende_block_info: &BlockInfo,
        l2_gas_used: GasAmount,
        block_header_commitments: BlockHeaderCommitments,
    ) -> Result<(), StateSyncClientError> {
        // Divide transactions hashes to L1Handler and RpcTransaction hashes.
        let account_transaction_hashes = transactions
            .iter()
            .filter_map(|tx| match tx.transaction {
                InternalConsensusTransaction::RpcTransaction(_) => Some(tx.transaction.tx_hash()),
                _ => None,
            })
            .collect::<Vec<TransactionHash>>();
        let l1_transaction_hashes = transactions
            .iter()
            .filter_map(|tx| match tx.transaction {
                InternalConsensusTransaction::L1Handler(_) => Some(tx.transaction.tx_hash()),
                _ => None,
            })
            .collect::<Vec<TransactionHash>>();

        let l1_gas_price = cende_block_info.gas_prices.l1_gas_price_per_token();
        let l1_data_gas_price = cende_block_info.gas_prices.l1_data_gas_price_per_token();
        let l2_gas_price = cende_block_info.gas_prices.l2_gas_price_per_token();
        let sequencer = SequencerContractAddress(init.builder);

        let block_header_without_hash = BlockHeaderWithoutHash {
            block_number: height,
            l1_gas_price,
            l1_data_gas_price,
            l2_gas_price,
            l2_gas_consumed: l2_gas_used,
            next_l2_gas_price: self.l2_gas_price,
            sequencer,
            timestamp: BlockTimestamp(init.timestamp),
            l1_da_mode: init.l1_da_mode,
            // TODO(guy.f): Figure out where/if to get the values below from and fill them.
            ..Default::default()
        };

        let sync_block = SyncBlock {
            state_diff: state_diff.clone(),
            account_transaction_hashes,
            l1_transaction_hashes,
            block_header_without_hash,
            block_header_commitments: Some(block_header_commitments),
        };

        self.deps.state_sync_client.add_new_block(sync_block).await
    }

    fn update_l2_gas_price(&mut self, height: BlockNumber, l2_gas_used: GasAmount) {
        if let Some(override_value) = self.config.dynamic_config.override_l2_gas_price_fri {
            info!(
                "L2 gas price ({}) is not updated, remains on override value of {override_value} \
                 fri",
                self.l2_gas_price.0
            );
            self.l2_gas_price = GasPrice(override_value);
        } else {
            let versioned_constants = VersionedConstants::latest_constants();
            let gas_target = versioned_constants.gas_target;

            let min_l2_gas_price_per_height =
                &self.config.dynamic_config.min_l2_gas_price_per_height;

            let min_gas_price = get_min_gas_price_for_height(height, min_l2_gas_price_per_height);
            self.l2_gas_price = calculate_next_base_gas_price(
                self.l2_gas_price,
                l2_gas_used,
                gas_target,
                min_gas_price,
            );
        }

        let gas_price_u64 = u64::try_from(self.l2_gas_price.0).unwrap_or(u64::MAX);
        CONSENSUS_L2_GAS_PRICE.set_lossy(gas_price_u64);
    }

    async fn finalize_decision(
        &mut self,
        height: BlockNumber,
        init: &ProposalInit,
        commitment: ProposalCommitment,
        // Accepts transactions as a vector of batches, as stored in the `BuiltProposals` map.
        transactions: Vec<Vec<InternalConsensusTransaction>>,
        decision_reached_response: DecisionReachedResponse,
    ) {
        let DecisionReachedResponse {
            state_diff,
            l2_gas_used,
            central_objects,
            block_header_commitments,
        } = decision_reached_response;

        self.update_l2_gas_price(height, l2_gas_used);

        // A hash map of (possibly failed) transactions, where the key is the transaction hash
        // and the value is the transaction itself.
        let mut transactions_hash_map = HashMap::new();
        for tx in transactions.into_iter().flatten() {
            let key = tx.tx_hash();
            if transactions_hash_map.insert(key, tx).is_some() {
                // TODO(Dafna): Handle this error gracefully.
                panic!("Duplicate transactions found with the same tx_hash: {key:?}");
            }
        }

        // Convert the execution infos to `InternalTransactionWithReceipt` format.
        // This is done by matching the transaction hashes in IndexMap<tx hash,execution info> with
        // the transactions returned by the batcher.
        //
        // Only successfully executed transactions will have execution infos.
        //
        // This data structure preserves the order of transactions as they were listed in
        // execution_infos.
        let transactions_with_execution_infos = central_objects
            .execution_infos
            .into_iter()
            .map(|(tx_hash, execution_info)| match transactions_hash_map.remove(&tx_hash) {
                Some(tx) => InternalTransactionWithReceipt { transaction: tx, execution_info },
                None => {
                    // TODO(Dafna): Handle this error gracefully.
                    panic!("Failed to find transaction for execution info with hash {tx_hash:?}.")
                }
            })
            .collect::<Vec<_>>();

        // The conversion should never fail, if we already managed to get a decision.
        let Ok(cende_block_info) = convert_to_sn_api_block_info(init) else {
            warn!("Failed to convert block info to SN API block info at height {height}: {init:?}");
            return;
        };

        if let Err(e) = self
            .update_state_sync_with_new_block(
                height,
                &state_diff,
                &transactions_with_execution_infos,
                init,
                &cende_block_info,
                l2_gas_used,
                block_header_commitments,
            )
            .await
        {
            // TODO(Shahak): Decide how to handle this error once p2p state sync is
            // production-ready. At this point, the block has already been committed to
            // the state.
            warn!("Failed to update state sync with new block at height {height}: {e:?}");
        }

        if let Err(e) = self
            .deps
            .cende_ambassador
            .prepare_blob_for_next_height(BlobParameters {
                block_info: cende_block_info,
                state_diff,
                compressed_state_diff: central_objects.compressed_state_diff,
                transactions_with_execution_infos,
                bouncer_weights: central_objects.bouncer_weights,
                casm_hash_computation_data_sierra_gas: central_objects
                    .casm_hash_computation_data_sierra_gas,
                casm_hash_computation_data_proving_gas: central_objects
                    .casm_hash_computation_data_proving_gas,
                fee_market_info: FeeMarketInfo {
                    l2_gas_consumed: l2_gas_used,
                    next_l2_gas_price: self.l2_gas_price,
                },
                compiled_class_hashes_for_migration: central_objects
                    .compiled_class_hashes_for_migration,
                proposal_commitment: commitment,
                parent_proposal_commitment: central_objects
                    .parent_proposal_commitment
                    .map(|commitment| ProposalCommitment(commitment.state_diff_commitment.0.0)),
            })
            .await
        {
            error!("Failed to prepare blob for next height at height {height}: {e:?}");
        }
    }

    pub fn get_config(&self) -> &ContextConfig {
        &self.config
    }
}

#[async_trait]
impl ConsensusContext for SequencerConsensusContext {
    type SignedProposalPart = SignedProposalPart;

    #[instrument(skip_all)]
    async fn build_proposal(
        &mut self,
        build_param: BuildParam,
        timeout: Duration,
    ) -> Result<oneshot::Receiver<ProposalCommitment>, ConsensusError> {
        let cende_write_success = if self.can_skip_write_prev_height_blob(build_param.height).await
        {
            // cende_write_success is a AbortOnDropHandle. To get the actual handle we need to
            // spawn the task.
            AbortOnDropHandle::new(tokio::spawn(ready(true)))
        } else {
            // TODO(dvir): consider start writing the blob in `decision_reached`, to reduce
            // transactions finality time. Use this option only for one special
            // sequencer that is the same cluster as the recorder.
            AbortOnDropHandle::new(
                self.deps.cende_ambassador.write_prev_height_blob(build_param.height),
            )
        };

        // Handles interrupting an active proposal from a previous height/round
        self.set_height_and_round(build_param.height, build_param.round).await?;
        assert!(
            self.active_proposal.is_none(),
            "We should not have an existing active proposal for the (height, round) when \
             build_proposal is called."
        );

        let (fin_sender, fin_receiver) = oneshot::channel();
        let proposal_id = ProposalId(self.proposal_id);
        self.proposal_id += 1;
        assert!(timeout > self.config.static_config.build_proposal_margin_millis);
        let stream_id = HeightAndRound(build_param.height.0, build_param.round);
        let stream_sender = self.start_stream(stream_id).await;

        info!(?build_param, ?timeout, %proposal_id, "Start building proposal");
        let cancel_token = CancellationToken::new();
        let cancel_token_clone = cancel_token.clone();
        let gas_price_params = make_gas_price_params(&self.config.dynamic_config);
        let mut l2_gas_price = self.l2_gas_price;
        if let Some(override_value) = self.config.dynamic_config.override_l2_gas_price_fri {
            info!("Overriding L2 gas price to {override_value} fri");
            l2_gas_price = GasPrice(override_value);
        }

        // The following calculations will panic on overflow/negative result.
        let total_build_proposal_time =
            timeout - self.config.static_config.build_proposal_margin_millis;
        let time_now = self.deps.clock.now();
        let batcher_deadline = time_now + total_build_proposal_time;
        let retrospective_block_hash_deadline = time_now
            + total_build_proposal_time.mul_f32(
                self.config.static_config.build_proposal_time_ratio_for_retrospective_block_hash,
            );

        let use_state_sync_block_timestamp =
            self.config.static_config.deployment_mode.use_state_sync_block_timestamp();

        let round = build_param.round;
        let args = ProposalBuildArguments {
            deps: self.deps.clone(),
            batcher_deadline,
            build_param,
            l1_da_mode: self.l1_da_mode,
            stream_sender,
            gas_price_params,
            valid_proposals: Arc::clone(&self.valid_proposals),
            proposal_id,
            cende_write_success,
            l2_gas_price,
            // TODO(Asmaa): Get it from committee once we have it.
            builder_address: self.config.static_config.builder_address,
            cancel_token,
            previous_block_info: self.previous_block_info.clone(),
            proposal_round: self.current_round,
            retrospective_block_hash_deadline,
            retrospective_block_hash_retry_interval_millis: self
                .config
                .static_config
                .retrospective_block_hash_retry_interval_millis,
            use_state_sync_block_timestamp,
        };

        let handle = tokio::spawn(
            async move {
                let res = build_proposal(args).await.map(|proposal_commitment| {
                    fin_sender.send(proposal_commitment).map_err(|e| {
                        BuildProposalError::SendError(format!(
                            "Failed to send proposal commitment: {e:?}"
                        ))
                    })?;
                    Ok::<_, BuildProposalError>(proposal_commitment)
                });
                match res {
                    Ok(proposal_commitment) => {
                        info!(?proposal_id, ?proposal_commitment, "Proposal succeeded.");
                    }
                    Err(e) => {
                        warn!("PROPOSAL_FAILED: Proposal failed as proposer. Error: {e:?}");
                        record_build_proposal_failure(e.into());
                    }
                }
            }
            .instrument(error_span!("consensus_build_proposal", %proposal_id, round)),
        );
        assert!(self.active_proposal.is_none());
        self.active_proposal = Some((cancel_token_clone, handle));

        Ok(fin_receiver)
    }

    #[instrument(skip_all)]
    async fn validate_proposal(
        &mut self,
        init: ProposalInit,
        timeout: Duration,
        content_receiver: mpsc::Receiver<Self::SignedProposalPart>,
    ) -> oneshot::Receiver<ProposalCommitment> {
        assert_eq!(Some(init.height), self.current_height);
        let (fin_sender, fin_receiver) = oneshot::channel();
        match init.round.cmp(&self.current_round) {
            std::cmp::Ordering::Less => {
                trace!("Dropping proposal from past round");
                fin_receiver
            }
            std::cmp::Ordering::Greater => {
                trace!("Queueing proposal for future round.");
                self.queued_proposals
                    .insert(init.round, ((init, timeout, content_receiver), fin_sender));
                fin_receiver
            }
            std::cmp::Ordering::Equal => {
                let block_info_validation = BlockInfoValidation {
                    height: init.height,
                    block_timestamp_window_seconds: self
                        .config
                        .static_config
                        .block_timestamp_window_seconds,
                    previous_block_info: self.previous_block_info.clone(),
                    l1_da_mode: self.l1_da_mode,
                    l2_gas_price_fri: self
                        .config
                        .dynamic_config
                        .override_l2_gas_price_fri
                        .map(GasPrice)
                        .unwrap_or(self.l2_gas_price),
                };
                self.validate_current_round_proposal(
                    init,
                    block_info_validation,
                    timeout,
                    self.config.static_config.validate_proposal_margin_millis,
                    content_receiver,
                    fin_sender,
                )
                .await;
                fin_receiver
            }
        }
    }

    async fn repropose(&mut self, id: ProposalCommitment, build_param: BuildParam) {
        info!(?id, ?build_param, "Reproposing.");
        let height = build_param.height;
        let (init, txs, _) = self
            .valid_proposals
            .lock()
            .expect("Lock on active proposals was poisoned due to a previous panic")
            .get_proposal(&height, &id)
            .clone();

        let transaction_converter = self.deps.transaction_converter.clone();
        let mut stream_sender =
            self.start_stream(HeightAndRound(height.0, build_param.round)).await;
        tokio::spawn(
            async move {
                let res =
                    send_reproposal(id, init, txs, &mut stream_sender, transaction_converter).await;
                match res {
                    Ok(()) => {
                        info!(?id, ?build_param, "Reproposal succeeded.");
                    }
                    Err(e) => {
                        warn!("REPROPOSE_FAILED: Reproposal failed. Error: {e:?}");
                    }
                }
            }
            .instrument(error_span!("consensus_repropose", round = build_param.round)),
        );
    }

    async fn broadcast(&mut self, message: Vote) -> Result<(), ConsensusError> {
        trace!("Broadcasting message: {message:?}");
        // Can fail only if the channel is disconnected, which should never happen.
        self.deps
            .vote_broadcast_client
            .broadcast_message(message)
            .await
            .map_err(|e| ConsensusError::InternalNetworkError(e.to_string()))
    }

    async fn decision_reached(
        &mut self,
        height: BlockNumber,
        commitment: ProposalCommitment,
    ) -> Result<(), ConsensusError> {
        info!("Finished consensus for height: {height}. Agreed on block: {:#066x}", commitment.0);

        self.interrupt_active_proposal().await;
        let proposal_id;
        let transactions;
        let init;
        {
            let mut proposals = self.valid_proposals.lock().unwrap();
            (init, transactions, proposal_id) =
                proposals.get_proposal(&height, &commitment).clone();

            proposals.remove_proposals_below_or_at_height(&height);
        }

        let decision_reached_response =
            self.deps.batcher.decision_reached(DecisionReachedInput { proposal_id }).await?;

        // CRITICAL: The block is now committed. This function must not fail beyond this point
        // unless the state is fully reverted, otherwise the node will be left in an
        // inconsistent state.

        self.finalize_decision(height, &init, commitment, transactions, decision_reached_response)
            .await;

        self.previous_block_info = Some(PreviousBlockInfo::from(&init));

        Ok(())
    }

    async fn try_sync(&mut self, height: BlockNumber) -> bool {
        let sync_block = match self.deps.state_sync_client.get_block(height).await {
            Err(StateSyncClientError::StateSyncError(StateSyncError::BlockNotFound(_))) => {
                return false;
            }
            Err(e) => {
                error!("Sync returned an error: {e:?}");
                return false;
            }
            Ok(block) => block,
        };
        // May be default for blocks older than 0.14.0, ensure min gas price is met.
        self.l2_gas_price = max(
            sync_block.block_header_without_hash.next_l2_gas_price,
            VersionedConstants::latest_constants().min_gas_price,
        );
        // TODO(Asmaa): validate starknet_version and parent_hash when they are stored.
        let block_number = sync_block.block_header_without_hash.block_number;
        let timestamp = sync_block.block_header_without_hash.timestamp;
        let last_block_timestamp =
            self.previous_block_info.as_ref().map_or(0, |info| info.timestamp);
        let now: u64 = self.deps.clock.unix_now();
        if !(block_number == height
            && timestamp.0 >= last_block_timestamp
            && timestamp.0 <= now + self.config.static_config.block_timestamp_window_seconds)
        {
            warn!(
                "Invalid block info: expected block number {}, got {}, expected timestamp range \
                 [{}, {}], got {}",
                height,
                block_number,
                last_block_timestamp,
                now + self.config.static_config.block_timestamp_window_seconds,
                timestamp.0,
            );
            return false;
        }
        self.previous_block_info =
            Some(previous_block_info_from_block_header(&sync_block.block_header_without_hash));
        self.interrupt_active_proposal().await;

        info!(
            "Adding sync block to Batcher for height {}",
            sync_block.block_header_without_hash.block_number,
        );
        if let Err(e) = self.deps.batcher.add_sync_block(sync_block).await {
            error!("Failed to add sync block to Batcher: {e:?}");
            return false;
        }

        true
    }

    async fn set_height_and_round(
        &mut self,
        height: BlockNumber,
        round: Round,
    ) -> Result<(), ConsensusError> {
        if self.current_height.map(|h| height > h).unwrap_or(true) {
            self.update_dynamic_config().await;
            self.current_height = Some(height);
            self.current_round = round;
            self.queued_proposals.clear();
            // The Batcher must be told when we begin to work on a new height. The implicit model is
            // that consensus works on a given height until it is done (either a decision is reached
            // or sync causes us to move on) and then moves on to a different height, never to
            // return to the old height.
            return Ok(self.deps.batcher.start_height(StartHeightInput { height }).await?);
        }
        assert_eq!(
            Some(height),
            self.current_height,
            "height {} is not equal to current height {:?}",
            height,
            self.current_height
        );
        if round == self.current_round {
            return Ok(());
        }
        assert!(
            round > self.current_round,
            "round {} is not greater than current round {}",
            round,
            self.current_round
        );
        self.interrupt_active_proposal().await;
        self.current_round = round;
        self.update_dynamic_config().await;

        let mut to_process = None;
        while let Some(entry) = self.queued_proposals.first_entry() {
            match self.current_round.cmp(entry.key()) {
                std::cmp::Ordering::Less => {
                    entry.remove();
                }
                std::cmp::Ordering::Equal => {
                    to_process = Some(entry.remove());
                    break;
                }
                std::cmp::Ordering::Greater => return Ok(()),
            }
        }
        // Validate the proposal for the current round if exists.
        let Some(((init, timeout, content), fin_sender)) = to_process else {
            return Ok(());
        };
        let block_info_validation = BlockInfoValidation {
            height: init.height,
            block_timestamp_window_seconds: self
                .config
                .static_config
                .block_timestamp_window_seconds,
            previous_block_info: self.previous_block_info.clone(),
            l1_da_mode: self.l1_da_mode,
            l2_gas_price_fri: self
                .config
                .dynamic_config
                .override_l2_gas_price_fri
                .map(GasPrice)
                .unwrap_or(self.l2_gas_price),
        };
        self.validate_current_round_proposal(
            init,
            block_info_validation,
            timeout,
            self.config.static_config.validate_proposal_margin_millis,
            content,
            fin_sender,
        )
        .await;
        Ok(())
    }
}

impl SequencerConsensusContext {
    async fn validate_current_round_proposal(
        &mut self,
        init: ProposalInit,
        block_info_validation: BlockInfoValidation,
        timeout: Duration,
        batcher_timeout_margin: Duration,
        content_receiver: mpsc::Receiver<SignedProposalPart>,
        fin_sender: oneshot::Sender<ProposalCommitment>,
    ) {
        let proposal_id = ProposalId(self.proposal_id);
        self.proposal_id += 1;
        info!(?timeout, %proposal_id, proposer=%init.proposer, round=self.current_round, "Start validating proposal");

        let cancel_token = CancellationToken::new();
        let cancel_token_clone = cancel_token.clone();
        let gas_price_params = make_gas_price_params(&self.config.dynamic_config);
        let args = ProposalValidateArguments {
            deps: self.deps.clone(),
            init,
            block_info_validation,
            proposal_id,
            timeout,
            batcher_timeout_margin,
            valid_proposals: Arc::clone(&self.valid_proposals),
            content_receiver,
            gas_price_params,
            cancel_token: cancel_token_clone,
        };

        let handle = tokio::spawn(
            async move {
                match validate_and_send(args, fin_sender).await {
                    Ok(proposal_commitment) => {
                        info!(?proposal_id, ?proposal_commitment, "Proposal succeeded.");
                    }
                    Err(e) => {
                        warn!("PROPOSAL_FAILED: Proposal failed as validator. Error: {e:?}");
                        record_validate_proposal_failure(e.into());
                    }
                }
            }
            .instrument(
                error_span!("consensus_validate_proposal", %proposal_id, round=self.current_round),
            ),
        );
        self.active_proposal = Some((cancel_token, handle));
    }

    async fn interrupt_active_proposal(&mut self) {
        if let Some((token, handle)) = self.active_proposal.take() {
            token.cancel();
            if let Err(e) = handle.await {
                warn!("Proposal task finished unexpectedly: {e:?}");
            }
        }
    }

    async fn update_dynamic_config(&mut self) {
        if let Some(config_manager_client) = self.deps.config_manager_client.clone() {
            let config_result = config_manager_client.get_context_dynamic_config().await;
            match config_result {
                Ok(config) => {
                    self.config.dynamic_config = config;
                }
                Err(e) => {
                    error!(
                        "Failed to get dynamic config for consensus context. Config not updated. \
                         Error: {e:?}"
                    );
                }
            }
        }
    }
}

async fn validate_and_send(
    args: ProposalValidateArguments,
    fin_sender: oneshot::Sender<ProposalCommitment>,
) -> Result<ProposalCommitment, ValidateProposalError> {
    let proposal_commitment = validate_proposal(args).await?;
    fin_sender
        .send(proposal_commitment)
        .map_err(|_| ValidateProposalError::SendError(proposal_commitment))?;
    Ok(proposal_commitment)
}

async fn send_reproposal(
    id: ProposalCommitment,
    init: ProposalInit,
    txs: Vec<Vec<InternalConsensusTransaction>>,
    stream_sender: &mut StreamSender,
    transaction_converter: Arc<dyn TransactionConverterTrait>,
) -> Result<(), ReproposeError> {
    stream_sender.send(SignedProposalPart::init(init)).await?;
    let mut n_executed_txs: usize = 0;
    for batch in txs.iter() {
        let transactions = futures::future::join_all(batch.iter().map(|tx| {
            // transaction_converter is an external dependency (class manager) and so
            // we can't assume success on reproposal.
            transaction_converter.convert_internal_consensus_tx_to_consensus_tx(tx.clone())
        }))
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
        stream_sender
            .send(SignedProposalPart::transactions(TransactionBatch { transactions }))
            .await?;
        n_executed_txs += batch.len();
    }
    let executed_transaction_count: u64 =
        n_executed_txs.try_into().expect("Number of executed transactions should fit in u64");
    let fin =
        ProposalFin { proposal_commitment: id, executed_transaction_count, commitment_parts: None };
    stream_sender.send(SignedProposalPart::fin(fin)).await?;

    Ok(())
}

fn previous_block_info_from_block_header(
    block_header: &BlockHeaderWithoutHash,
) -> PreviousBlockInfo {
    PreviousBlockInfo {
        timestamp: block_header.timestamp.0,
        l1_prices_wei: L1PricesInWei {
            l1_gas_price: block_header.l1_gas_price.price_in_wei,
            l1_data_gas_price: block_header.l1_data_gas_price.price_in_wei,
        },
        l1_prices_fri: L1PricesInFri {
            l1_gas_price: block_header.l1_gas_price.price_in_fri,
            l1_data_gas_price: block_header.l1_data_gas_price.price_in_fri,
        },
    }
}
