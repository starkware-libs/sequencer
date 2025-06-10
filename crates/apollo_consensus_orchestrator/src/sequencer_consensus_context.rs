//! Implementation of the ConsensusContext interface for running the sequencer.
//!
//! It connects to the Batcher who is responsible for building/validating blocks.
#[cfg(test)]
#[path = "sequencer_consensus_context_test.rs"]
mod sequencer_consensus_context_test;

use std::cmp::max;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use apollo_batcher_types::batcher_types::{
    DecisionReachedInput,
    DecisionReachedResponse,
    GetProposalContent,
    GetProposalContentInput,
    ProposalId,
    ProposalStatus,
    ProposeBlockInput,
    SendProposalContent,
    SendProposalContentInput,
    StartHeightInput,
    ValidateBlockInput,
};
use apollo_batcher_types::communication::{BatcherClient, BatcherClientError};
use apollo_class_manager_types::transaction_converter::TransactionConverterTrait;
use apollo_consensus::types::{
    ConsensusContext,
    ConsensusError,
    ProposalCommitment,
    Round,
    ValidatorId,
};
use apollo_l1_gas_price_types::errors::{EthToStrkOracleClientError, L1GasPriceClientError};
use apollo_l1_gas_price_types::{EthToStrkOracleClientTrait, L1GasPriceProviderClient};
use apollo_network::network_manager::{BroadcastTopicClient, BroadcastTopicClientTrait};
use apollo_protobuf::consensus::{
    ConsensusBlockInfo,
    HeightAndRound,
    ProposalFin,
    ProposalInit,
    ProposalPart,
    TransactionBatch,
    Vote,
    DEFAULT_VALIDATOR_ID,
};
use apollo_state_sync_types::communication::{StateSyncClient, StateSyncClientError};
use apollo_state_sync_types::state_sync_types::SyncBlock;
use apollo_time::time_keeper::{sleep_until, Clock, DateTime};
use async_trait::async_trait;
// TODO(Gilad): Define in consensus, either pass to blockifier as config or keep the dup.
use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
use futures::channel::{mpsc, oneshot};
use futures::{FutureExt, SinkExt, StreamExt};
use num_rational::Ratio;
use starknet_api::block::{
    BlockHash,
    BlockHashAndNumber,
    BlockHeaderWithoutHash,
    BlockNumber,
    BlockTimestamp,
    GasPrice,
    GasPricePerToken,
    GasPriceVector,
    GasPrices,
    NonzeroGasPrice,
    WEI_PER_ETH,
};
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::core::{ContractAddress, SequencerContractAddress};
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::execution_resources::GasAmount;
use starknet_api::transaction::TransactionHash;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio_util::task::AbortOnDropHandle;
use tracing::{debug, error, error_span, info, instrument, trace, warn, Instrument};

use crate::cende::{BlobParameters, CendeContext};
use crate::config::ContextConfig;
use crate::fee_market::{calculate_next_base_gas_price, FeeMarketInfo};
use crate::metrics::{
    register_metrics,
    CONSENSUS_L1_DATA_GAS_MISMATCH,
    CONSENSUS_L1_GAS_MISMATCH,
    CONSENSUS_L2_GAS_PRICE,
    CONSENSUS_NUM_BATCHES_IN_PROPOSAL,
    CONSENSUS_NUM_TXS_IN_PROPOSAL,
};
use crate::orchestrator_versioned_constants::VersionedConstants;
use crate::utils::{get_oracle_rate_and_prices, GasPriceParams};

// Contains parameters required for validating block info.
#[derive(Clone, Debug)]
struct BlockInfoValidation {
    height: BlockNumber,
    block_timestamp_window_seconds: u64,
    previous_block_info: Option<ConsensusBlockInfo>,
    l1_da_mode: L1DataAvailabilityMode,
    l2_gas_price_fri: GasPrice,
}

type ValidationParams = (BlockNumber, ValidatorId, Duration, mpsc::Receiver<ProposalPart>);
type ProposalResult<T> = Result<T, BuildProposalError>;

enum HandledProposalPart {
    Continue,
    Invalid,
    Finished(ProposalCommitment, ProposalFin),
    Failed(String),
}

#[derive(Debug, thiserror::Error)]
enum BuildProposalError {
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
}

type HeightToIdToContent = BTreeMap<
    BlockNumber,
    BTreeMap<
        ProposalCommitment,
        (ConsensusBlockInfo, Vec<Vec<InternalConsensusTransaction>>, ProposalId),
    >,
>;

struct BuiltProposals {
    // {height: {proposal_commitment: (block_info, content, [proposal_ids])}}
    // Note that multiple proposals IDs can be associated with the same content, but we only need
    // to store one of them.
    //
    // The tranasactions are stored as a vector of batches (as returned from the batcher) and not
    // flattened. This is since we might need to repropose, in which case we need to send the
    // transactions in batches.
    data: HeightToIdToContent,
}

impl BuiltProposals {
    fn new() -> Self {
        Self { data: HeightToIdToContent::default() }
    }

    fn get_proposal(
        &self,
        height: &BlockNumber,
        commitment: &ProposalCommitment,
    ) -> &(ConsensusBlockInfo, Vec<Vec<InternalConsensusTransaction>>, ProposalId) {
        self.data
            .get(height)
            .unwrap_or_else(|| panic!("No proposals found for height {height}"))
            .get(commitment)
            .unwrap_or_else(|| panic!("No proposal found for height {height} and id {commitment}"))
    }

    fn remove_proposals_below_or_at_height(&mut self, height: &BlockNumber) {
        self.data.retain(|&h, _| h > *height);
    }

    fn insert_proposal_for_height(
        &mut self,
        height: &BlockNumber,
        proposal_commitment: &ProposalCommitment,
        block_info: ConsensusBlockInfo,
        transactions: Vec<Vec<InternalConsensusTransaction>>,
        proposal_id: &ProposalId,
    ) {
        self.data
            .entry(*height)
            .or_default()
            .insert(*proposal_commitment, (block_info, transactions, *proposal_id));
    }
}

pub struct SequencerConsensusContext {
    config: ContextConfig,
    deps: SequencerConsensusContextDeps,
    validators: Vec<ValidatorId>,
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
    previous_block_info: Option<ConsensusBlockInfo>,
}

#[derive(Clone)]
pub struct SequencerConsensusContextDeps {
    pub transaction_converter: Arc<dyn TransactionConverterTrait>,
    pub state_sync_client: Arc<dyn StateSyncClient>,
    pub batcher: Arc<dyn BatcherClient>,
    pub cende_ambassador: Arc<dyn CendeContext>,
    pub eth_to_strk_oracle_client: Arc<dyn EthToStrkOracleClientTrait>,
    pub l1_gas_price_provider: Arc<dyn L1GasPriceProviderClient>,
    /// Use DefaultClock if you don't want to inject timestamps.
    pub clock: Arc<dyn Clock>,
    // Used to initiate new outbound proposal streams.
    pub outbound_proposal_sender: mpsc::Sender<(HeightAndRound, mpsc::Receiver<ProposalPart>)>,
    // Used to broadcast votes to other consensus nodes.
    pub vote_broadcast_client: BroadcastTopicClient<Vote>,
}

impl SequencerConsensusContext {
    pub fn new(config: ContextConfig, deps: SequencerConsensusContextDeps) -> Self {
        register_metrics();
        let num_validators = config.num_validators;
        let l1_da_mode = if config.l1_da_mode {
            L1DataAvailabilityMode::Blob
        } else {
            L1DataAvailabilityMode::Calldata
        };
        Self {
            config,
            deps,
            // TODO(Matan): Set the actual validator IDs (contract addresses).
            validators: (0..num_validators)
                .map(|i| ValidatorId::from(DEFAULT_VALIDATOR_ID + i))
                .collect(),
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
}

struct ProposalBuildArguments {
    deps: SequencerConsensusContextDeps,
    batcher_timeout: Duration,
    proposal_init: ProposalInit,
    l1_da_mode: L1DataAvailabilityMode,
    proposal_sender: mpsc::Sender<ProposalPart>,
    fin_sender: oneshot::Sender<ProposalCommitment>,
    gas_price_params: GasPriceParams,
    valid_proposals: Arc<Mutex<BuiltProposals>>,
    proposal_id: ProposalId,
    cende_write_success: AbortOnDropHandle<bool>,
    l2_gas_price: GasPrice,
    builder_address: ContractAddress,
    cancel_token: CancellationToken,
    previous_block_info: Option<ConsensusBlockInfo>,
    proposal_round: Round,
}

struct ProposalValidateArguments {
    deps: SequencerConsensusContextDeps,
    block_info_validation: BlockInfoValidation,
    proposal_id: ProposalId,
    timeout: Duration,
    batcher_timeout_margin: Duration,
    valid_proposals: Arc<Mutex<BuiltProposals>>,
    content_receiver: mpsc::Receiver<ProposalPart>,
    fin_sender: oneshot::Sender<ProposalCommitment>,
    gas_price_params: GasPriceParams,
    cancel_token: CancellationToken,
}

#[async_trait]
impl ConsensusContext for SequencerConsensusContext {
    type ProposalPart = ProposalPart;

    #[instrument(skip_all)]
    async fn build_proposal(
        &mut self,
        proposal_init: ProposalInit,
        timeout: Duration,
    ) -> oneshot::Receiver<ProposalCommitment> {
        // TODO(dvir): consider start writing the blob in `decision_reached`, to reduce transactions
        // finality time. Use this option only for one special sequencer that is the same cluster as
        // the recorder.
        let cende_write_success = AbortOnDropHandle::new(
            self.deps.cende_ambassador.write_prev_height_blob(proposal_init.height),
        );
        // Handles interrupting an active proposal from a previous height/round
        self.set_height_and_round(proposal_init.height, proposal_init.round).await;
        assert!(
            self.active_proposal.is_none(),
            "We should not have an existing active proposal for the (height, round) when \
             build_proposal is called."
        );

        let (fin_sender, fin_receiver) = oneshot::channel();
        let proposal_id = ProposalId(self.proposal_id);
        self.proposal_id += 1;
        assert!(timeout > self.config.build_proposal_margin_millis);
        let (proposal_sender, proposal_receiver) = mpsc::channel(self.config.proposal_buffer_size);
        let stream_id = HeightAndRound(proposal_init.height.0, proposal_init.round);
        self.deps
            .outbound_proposal_sender
            .send((stream_id, proposal_receiver))
            .await
            .expect("Failed to send proposal receiver");

        info!(?proposal_init, ?timeout, %proposal_id, "Building proposal");
        let cancel_token = CancellationToken::new();
        let cancel_token_clone = cancel_token.clone();
        let gas_price_params = GasPriceParams {
            min_l1_gas_price_wei: GasPrice(self.config.min_l1_gas_price_wei),
            max_l1_gas_price_wei: GasPrice(self.config.max_l1_gas_price_wei),
            min_l1_data_gas_price_wei: GasPrice(self.config.min_l1_data_gas_price_wei),
            max_l1_data_gas_price_wei: GasPrice(self.config.max_l1_data_gas_price_wei),
            l1_data_gas_price_multiplier: Ratio::new(
                self.config.l1_data_gas_price_multiplier_ppt,
                1000,
            ),
            l1_gas_tip_wei: GasPrice(self.config.l1_gas_tip_wei),
        };
        let args = ProposalBuildArguments {
            deps: self.deps.clone(),
            batcher_timeout: timeout - self.config.build_proposal_margin_millis,
            proposal_init,
            l1_da_mode: self.l1_da_mode,
            proposal_sender,
            fin_sender,
            gas_price_params,
            valid_proposals: Arc::clone(&self.valid_proposals),
            proposal_id,
            cende_write_success,
            l2_gas_price: self.l2_gas_price,
            builder_address: self.config.builder_address,
            cancel_token,
            previous_block_info: self.previous_block_info.clone(),
            proposal_round: self.current_round,
        };
        let handle = tokio::spawn(
            async move {
                build_proposal(args).await;
            }
            .instrument(
                error_span!("consensus_build_proposal", %proposal_id, round=proposal_init.round),
            ),
        );
        assert!(self.active_proposal.is_none());
        self.active_proposal = Some((cancel_token_clone, handle));

        fin_receiver
    }

    #[instrument(skip_all)]
    async fn validate_proposal(
        &mut self,
        proposal_init: ProposalInit,
        timeout: Duration,
        content_receiver: mpsc::Receiver<Self::ProposalPart>,
    ) -> oneshot::Receiver<ProposalCommitment> {
        assert_eq!(Some(proposal_init.height), self.current_height);
        let (fin_sender, fin_receiver) = oneshot::channel();
        match proposal_init.round.cmp(&self.current_round) {
            std::cmp::Ordering::Less => {
                trace!("Dropping proposal from past round");
                fin_receiver
            }
            std::cmp::Ordering::Greater => {
                trace!("Queueing proposal for future round.");
                self.queued_proposals.insert(
                    proposal_init.round,
                    (
                        (proposal_init.height, proposal_init.proposer, timeout, content_receiver),
                        fin_sender,
                    ),
                );
                fin_receiver
            }
            std::cmp::Ordering::Equal => {
                let block_info_validation = BlockInfoValidation {
                    height: proposal_init.height,
                    block_timestamp_window_seconds: self.config.block_timestamp_window_seconds,
                    previous_block_info: self.previous_block_info.clone(),
                    l1_da_mode: self.l1_da_mode,
                    l2_gas_price_fri: self.l2_gas_price,
                };
                self.validate_current_round_proposal(
                    block_info_validation,
                    proposal_init.proposer,
                    timeout,
                    self.config.validate_proposal_margin_millis,
                    content_receiver,
                    fin_sender,
                )
                .await;
                fin_receiver
            }
        }
    }

    async fn repropose(&mut self, id: ProposalCommitment, init: ProposalInit) {
        info!(?id, ?init, "Reproposing.");
        let height = init.height;
        let (block_info, txs, _) = self
            .valid_proposals
            .lock()
            .expect("Lock on active proposals was poisoned due to a previous panic")
            .get_proposal(&height, &id)
            .clone();

        let transaction_converter = self.deps.transaction_converter.clone();
        let mut outbound_proposal_sender = self.deps.outbound_proposal_sender.clone();
        let channel_size = self.config.proposal_buffer_size;
        tokio::spawn(
            async move {
                let (mut proposal_sender, proposal_receiver) = mpsc::channel(channel_size);
                let stream_id = HeightAndRound(height.0, init.round);
                outbound_proposal_sender
                    .send((stream_id, proposal_receiver))
                    .await
                    .expect("Failed to send proposal receiver");
                proposal_sender
                    .send(ProposalPart::Init(init))
                    .await
                    .expect("Failed to send proposal init");
                proposal_sender
                    .send(ProposalPart::BlockInfo(block_info.clone()))
                    .await
                    .expect("Failed to send block info");
                for batch in txs.iter() {
                    let transactions = futures::future::join_all(batch.iter().map(|tx| {
                        transaction_converter
                            .convert_internal_consensus_tx_to_consensus_tx(tx.clone())
                    }))
                    .await
                    .into_iter()
                    .collect::<Result<Vec<_>, _>>()
                    .expect("Failed converting transaction during repropose");

                    proposal_sender
                        .send(ProposalPart::Transactions(TransactionBatch { transactions }))
                        .await
                        .expect("Failed to broadcast proposal content");
                }
                proposal_sender
                    .send(ProposalPart::Fin(ProposalFin { proposal_commitment: id }))
                    .await
                    .expect("Failed to broadcast proposal fin");
            }
            .instrument(error_span!("consensus_repropose", round = init.round)),
        );
    }

    async fn validators(&self, _height: BlockNumber) -> Vec<ValidatorId> {
        self.validators.clone()
    }

    fn proposer(&self, height: BlockNumber, round: Round) -> ValidatorId {
        let height: usize = height.0.try_into().expect("Cannot convert to usize");
        let round: usize = round.try_into().expect("Cannot convert to usize");
        *self
            .validators
            .get((height + round) % self.validators.len())
            .expect("There should be at least one validator")
    }

    async fn broadcast(&mut self, message: Vote) -> Result<(), ConsensusError> {
        trace!("Broadcasting message: {message:?}");
        self.deps.vote_broadcast_client.broadcast_message(message).await?;
        Ok(())
    }

    async fn decision_reached(
        &mut self,
        block: ProposalCommitment,
        precommits: Vec<Vote>,
    ) -> Result<(), ConsensusError> {
        let height = precommits[0].height;
        info!("Finished consensus for height: {height}. Agreed on block: {:#064x}", block.0);

        self.interrupt_active_proposal().await;
        let proposal_id;
        let transactions;
        let block_info;
        {
            let height = BlockNumber(height);
            let mut proposals = self
                .valid_proposals
                .lock()
                .expect("Lock on active proposals was poisoned due to a previous panic");
            (block_info, transactions, proposal_id) =
                proposals.get_proposal(&height, &block).clone();

            proposals.remove_proposals_below_or_at_height(&height);
        }
        let transactions = transactions.concat();
        // TODO(dvir): return from the batcher's 'decision_reached' function the relevant data to
        // build a blob.
        let DecisionReachedResponse { state_diff, l2_gas_used, central_objects } = self
            .deps
            .batcher
            .decision_reached(DecisionReachedInput { proposal_id })
            .await
            .expect("Failed to get state diff.");

        let gas_target = GasAmount(VersionedConstants::latest_constants().max_block_size.0 / 2);
        self.l2_gas_price =
            calculate_next_base_gas_price(self.l2_gas_price, l2_gas_used, gas_target);

        let gas_price_u64 = u64::try_from(self.l2_gas_price.0).unwrap_or(u64::MAX);
        CONSENSUS_L2_GAS_PRICE.set_lossy(gas_price_u64);

        let cende_block_info = convert_to_sn_api_block_info(&block_info);
        let l1_gas_price = GasPricePerToken {
            price_in_fri: cende_block_info.gas_prices.strk_gas_prices.l1_gas_price.get(),
            price_in_wei: cende_block_info.gas_prices.eth_gas_prices.l1_gas_price.get(),
        };
        let l1_data_gas_price = GasPricePerToken {
            price_in_fri: cende_block_info.gas_prices.strk_gas_prices.l1_data_gas_price.get(),
            price_in_wei: cende_block_info.gas_prices.eth_gas_prices.l1_data_gas_price.get(),
        };
        let l2_gas_price = GasPricePerToken {
            price_in_fri: cende_block_info.gas_prices.strk_gas_prices.l2_gas_price.get(),
            price_in_wei: cende_block_info.gas_prices.eth_gas_prices.l2_gas_price.get(),
        };
        let sequencer = SequencerContractAddress(block_info.builder);

        let block_header_without_hash = BlockHeaderWithoutHash {
            block_number: BlockNumber(height),
            l1_gas_price,
            l1_data_gas_price,
            l2_gas_price,
            l2_gas_consumed: l2_gas_used,
            next_l2_gas_price: self.l2_gas_price,
            sequencer,
            timestamp: BlockTimestamp(block_info.timestamp),
            l1_da_mode: block_info.l1_da_mode,
            // TODO(guy.f): Figure out where/if to get the values below from and fill them.
            ..Default::default()
        };

        // Divide transactions hashes to L1Handler and RpcTransaction hashes.
        let account_transaction_hashes = transactions
            .iter()
            .filter_map(|tx| match tx {
                InternalConsensusTransaction::RpcTransaction(_) => Some(tx.tx_hash()),
                _ => None,
            })
            .collect::<Vec<TransactionHash>>();
        let l1_transaction_hashes = transactions
            .iter()
            .filter_map(|tx| match tx {
                InternalConsensusTransaction::L1Handler(_) => Some(tx.tx_hash()),
                _ => None,
            })
            .collect::<Vec<TransactionHash>>();

        let sync_block = SyncBlock {
            state_diff: state_diff.clone(),
            account_transaction_hashes,
            l1_transaction_hashes,
            block_header_without_hash,
        };
        let state_sync_client = self.deps.state_sync_client.clone();
        // `add_new_block` returns immediately, it doesn't wait for sync to fully process the block.
        state_sync_client.add_new_block(sync_block).await.expect("Failed to add new block.");

        // TODO(dvir): pass here real `BlobParameters` info.
        // TODO(dvir): when passing here the correct `BlobParameters`, also test that
        // `prepare_blob_for_next_height` is called with the correct parameters.
        let _ = self
            .deps
            .cende_ambassador
            .prepare_blob_for_next_height(BlobParameters {
                block_info: cende_block_info,
                state_diff,
                compressed_state_diff: central_objects.compressed_state_diff,
                transactions,
                execution_infos: central_objects.execution_infos,
                bouncer_weights: central_objects.bouncer_weights,
                casm_hash_computation_data: central_objects.casm_hash_computation_data,
                fee_market_info: FeeMarketInfo {
                    l2_gas_consumed: l2_gas_used,
                    next_l2_gas_price: self.l2_gas_price,
                },
            })
            .await
            .inspect_err(|e| {
                error!("Failed to prepare blob for next height: {e:?}");
            });
        self.previous_block_info = Some(block_info);
        Ok(())
    }

    async fn try_sync(&mut self, height: BlockNumber) -> bool {
        let sync_block = match self.deps.state_sync_client.get_block(height).await {
            Err(e) => {
                error!("Sync returned an error: {e:?}");
                return false;
            }
            Ok(None) => return false,
            Ok(Some(block)) => block,
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
            && timestamp.0 <= now + self.config.block_timestamp_window_seconds)
        {
            warn!(
                "Invalid block info: expected block number {}, got {}, expected timestamp range \
                 [{}, {}], got {}",
                height,
                block_number,
                last_block_timestamp,
                now + self.config.block_timestamp_window_seconds,
                timestamp.0,
            );
            return false;
        }
        let eth_to_fri_rate = sync_block
            .block_header_without_hash
            .l1_gas_price
            .price_in_fri
            .checked_mul_u128(WEI_PER_ETH)
            .expect("Gas price overflow")
            .checked_div(sync_block.block_header_without_hash.l1_gas_price.price_in_wei.0)
            .expect("Price in wei should be non-zero")
            .0;
        self.previous_block_info = Some(ConsensusBlockInfo {
            height,
            timestamp: timestamp.0,
            builder: sync_block.block_header_without_hash.sequencer.0,
            l1_da_mode: sync_block.block_header_without_hash.l1_da_mode,
            l2_gas_price_fri: sync_block.block_header_without_hash.l2_gas_price.price_in_fri,
            l1_gas_price_wei: sync_block.block_header_without_hash.l1_gas_price.price_in_wei,
            l1_data_gas_price_wei: sync_block
                .block_header_without_hash
                .l1_data_gas_price
                .price_in_wei,
            eth_to_fri_rate,
        });
        self.interrupt_active_proposal().await;
        self.deps.batcher.add_sync_block(sync_block).await.unwrap();
        true
    }

    async fn set_height_and_round(&mut self, height: BlockNumber, round: Round) {
        if self.current_height.map(|h| height > h).unwrap_or(true) {
            self.current_height = Some(height);
            assert_eq!(round, 0);
            self.current_round = round;
            self.queued_proposals.clear();
            // The Batcher must be told when we begin to work on a new height. The implicit model is
            // that consensus works on a given height until it is done (either a decision is reached
            // or sync causes us to move on) and then moves on to a different height, never to
            // return to the old height.
            self.deps
                .batcher
                .start_height(StartHeightInput { height })
                .await
                .expect("Batcher should be ready to start the next height");
            return;
        }
        assert_eq!(Some(height), self.current_height);
        if round == self.current_round {
            return;
        }
        assert!(round > self.current_round);
        self.interrupt_active_proposal().await;
        self.current_round = round;
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
                std::cmp::Ordering::Greater => return,
            }
        }
        // Validate the proposal for the current round if exists.
        let Some(((height, validator, timeout, content), fin_sender)) = to_process else {
            return;
        };
        let block_info_validation = BlockInfoValidation {
            height,
            block_timestamp_window_seconds: self.config.block_timestamp_window_seconds,
            previous_block_info: self.previous_block_info.clone(),
            l1_da_mode: self.l1_da_mode,
            l2_gas_price_fri: self.l2_gas_price,
        };
        self.validate_current_round_proposal(
            block_info_validation,
            validator,
            timeout,
            self.config.validate_proposal_margin_millis,
            content,
            fin_sender,
        )
        .await;
    }
}

impl SequencerConsensusContext {
    async fn validate_current_round_proposal(
        &mut self,
        block_info_validation: BlockInfoValidation,
        proposer: ValidatorId,
        timeout: Duration,
        batcher_timeout_margin: Duration,
        content_receiver: mpsc::Receiver<ProposalPart>,
        fin_sender: oneshot::Sender<ProposalCommitment>,
    ) {
        let cancel_token = CancellationToken::new();
        let cancel_token_clone = cancel_token.clone();
        let l1_gas_tip_wei = GasPrice(self.config.l1_gas_tip_wei);
        let valid_proposals = Arc::clone(&self.valid_proposals);
        let proposal_id = ProposalId(self.proposal_id);
        self.proposal_id += 1;
        let deps = self.deps.clone();
        let gas_price_params = GasPriceParams {
            min_l1_gas_price_wei: GasPrice(self.config.min_l1_gas_price_wei),
            max_l1_gas_price_wei: GasPrice(self.config.max_l1_gas_price_wei),
            min_l1_data_gas_price_wei: GasPrice(self.config.min_l1_data_gas_price_wei),
            max_l1_data_gas_price_wei: GasPrice(self.config.max_l1_data_gas_price_wei),
            l1_data_gas_price_multiplier: Ratio::new(
                self.config.l1_data_gas_price_multiplier_ppt,
                1000,
            ),
            l1_gas_tip_wei,
        };

        info!(?timeout, %proposal_id, %proposer, round=self.current_round, "Validating proposal.");
        let handle = tokio::spawn(
            async move {
                validate_proposal(ProposalValidateArguments {
                    deps,
                    block_info_validation,
                    proposal_id,
                    timeout,
                    batcher_timeout_margin,
                    valid_proposals,
                    content_receiver,
                    fin_sender,
                    gas_price_params,
                    cancel_token: cancel_token_clone,
                })
                .await
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
            handle.await.expect("Proposal task failed");
        }
    }
}

// Handles building a new proposal without blocking consensus:
async fn build_proposal(mut args: ProposalBuildArguments) {
    let block_info = initiate_build(&args).await;
    let block_info = match block_info {
        Ok(info) => info,
        Err(e) => {
            error!("Failed to initiate proposal build. {e:?}");
            return;
        }
    };
    args.proposal_sender
        .send(ProposalPart::Init(args.proposal_init))
        .await
        .expect("Failed to send proposal init");
    args.proposal_sender
        .send(ProposalPart::BlockInfo(block_info.clone()))
        .await
        .expect("Failed to send block info");

    let Some((proposal_commitment, content)) = get_proposal_content(
        args.proposal_id,
        args.deps.batcher.as_ref(),
        args.proposal_sender,
        args.cende_write_success,
        args.deps.transaction_converter,
        args.cancel_token,
    )
    .await
    else {
        return;
    };

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
    if args.fin_sender.send(proposal_commitment).is_err() {
        // Consensus may exit early (e.g. sync).
        warn!("Failed to send proposal content id");
    }
}

async fn initiate_build(args: &ProposalBuildArguments) -> ProposalResult<ConsensusBlockInfo> {
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
) -> Option<(ProposalCommitment, Vec<Vec<InternalConsensusTransaction>>)> {
    let mut content = Vec::new();
    loop {
        if cancel_token.is_cancelled() {
            warn!("Proposal interrupted during building.");
            return None;
        }
        // We currently want one part of the node failing to cause all components to fail. If this
        // changes, we can simply return None and consider this as a failed proposal which consensus
        // should support.
        let response = batcher.get_proposal_content(GetProposalContentInput { proposal_id }).await;
        let response = match response {
            Ok(resp) => resp,
            Err(e) => {
                error!("Failed to get proposal content. {e:?}");
                return None;
            }
        };

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
                .collect::<Result<Vec<_>, _>>();
                let transactions = match transactions {
                    Ok(txs) => txs,
                    Err(e) => {
                        error!("Failed to convert transactions. {e:?}");
                        return None;
                    }
                };

                trace!(?transactions, "Sending transaction batch with {} txs.", transactions.len());
                proposal_sender
                    .send(ProposalPart::Transactions(TransactionBatch { transactions }))
                    .await
                    .expect("Failed to broadcast proposal content");
            }
            GetProposalContent::Finished(id) => {
                let proposal_commitment = BlockHash(id.state_diff_commitment.0.0);
                let num_txs: usize = content.iter().map(|batch| batch.len()).sum();
                info!(?proposal_commitment, num_txs = num_txs, "Finished building proposal",);
                if num_txs == 0 {
                    warn!("Built an empty proposal.");
                }

                // If the blob writing operation to Aerospike doesn't return a success status, we
                // can't finish the proposal.
                match cende_write_success.now_or_never() {
                    Some(Ok(true)) => {
                        info!("Writing blob to Aerospike completed successfully.");
                    }
                    Some(Ok(false)) => {
                        warn!("Writing blob to Aerospike failed.");
                        return None;
                    }
                    Some(Err(e)) => {
                        warn!("Writing blob to Aerospike failed. Error: {e:?}");
                        return None;
                    }
                    None => {
                        warn!("Writing blob to Aerospike didn't return in time.");
                        return None;
                    }
                }

                let fin = ProposalFin { proposal_commitment };
                info!("Sending fin={fin:?}");
                proposal_sender
                    .send(ProposalPart::Fin(fin))
                    .await
                    .expect("Failed to broadcast proposal fin");
                return Some((proposal_commitment, content));
            }
        }
    }
}

async fn validate_proposal(mut args: ProposalValidateArguments) {
    let mut content = Vec::new();
    let now = args.deps.clock.now();

    let Some(deadline) = now.checked_add_signed(chrono::TimeDelta::from_std(args.timeout).unwrap())
    else {
        warn!("Cannot calculate deadline. Timeout: {:?}, now: {:?}", args.timeout, now);
        return;
    };

    let Some((block_info, fin_sender)) = await_second_proposal_part(
        &args.cancel_token,
        deadline,
        &mut args.content_receiver,
        args.fin_sender,
        args.deps.clock.as_ref(),
    )
    .await
    else {
        return;
    };
    if !is_block_info_valid(
        args.block_info_validation.clone(),
        block_info.clone(),
        args.deps.eth_to_strk_oracle_client,
        args.deps.clock.as_ref(),
        args.deps.l1_gas_price_provider,
        &args.gas_price_params,
    )
    .await
    {
        return;
    }
    if let Err(e) = initiate_validation(
        args.deps.batcher.as_ref(),
        args.deps.state_sync_client,
        block_info.clone(),
        args.proposal_id,
        args.timeout + args.batcher_timeout_margin,
        args.deps.clock.as_ref(),
    )
    .await
    {
        error!("Failed to initiate proposal validation. {e:?}");
        return;
    }
    // Validating the rest of the proposal parts.
    let (built_block, received_fin) = loop {
        tokio::select! {
            _ = args.cancel_token.cancelled() => {
                warn!("Proposal interrupted during validation.");
                batcher_abort_proposal(args.deps.batcher.as_ref(), args.proposal_id).await;
                return;
            }
            _ = sleep_until(deadline, args.deps.clock.as_ref()) => {
                warn!("Validation timed out.");
                batcher_abort_proposal(args.deps.batcher.as_ref(), args.proposal_id).await;
                return;
            }
            proposal_part = args.content_receiver.next() => {
                match handle_proposal_part(
                    args.proposal_id,
                    args.deps.batcher.as_ref(),
                    proposal_part,
                    &mut content,
                    args.deps.transaction_converter.clone(),
                ).await {
                    HandledProposalPart::Finished(built_block, received_fin) => {
                        break (built_block, received_fin);
                    }
                    HandledProposalPart::Continue => {continue;}
                    HandledProposalPart::Invalid => {
                        warn!("Invalid proposal.");
                        // No need to abort since the Batcher is the source of this info.
                        return;
                    }
                    HandledProposalPart::Failed(fail_reason) => {
                        warn!("Failed to handle proposal part. {fail_reason}");
                        batcher_abort_proposal(args.deps.batcher.as_ref(), args.proposal_id).await;
                        return;
                    }
                }
            }
        }
    };

    let num_txs: usize = content.iter().map(|batch| batch.len()).sum();
    CONSENSUS_NUM_BATCHES_IN_PROPOSAL.set_lossy(content.len());
    CONSENSUS_NUM_TXS_IN_PROPOSAL.set_lossy(num_txs);

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
        warn!("proposal_id built from content received does not match fin.");
        return;
    }

    if fin_sender.send(built_block).is_err() {
        // Consensus may exit early (e.g. sync).
        warn!("Failed to send proposal content ids");
    }
}

#[instrument(level = "warn", skip_all, fields(?block_info_validation, ?block_info_proposed))]
async fn is_block_info_valid(
    block_info_validation: BlockInfoValidation,
    block_info_proposed: ConsensusBlockInfo,
    eth_to_strk_oracle_client: Arc<dyn EthToStrkOracleClientTrait>,
    time: &dyn Clock,
    l1_gas_price_provider: Arc<dyn L1GasPriceProviderClient>,
    gas_price_params: &GasPriceParams,
) -> bool {
    let now: u64 = time.unix_now();
    let last_block_timestamp =
        block_info_validation.previous_block_info.as_ref().map_or(0, |info| info.timestamp);
    if !(block_info_proposed.height == block_info_validation.height
        && block_info_proposed.timestamp >= last_block_timestamp
        // Check timestamp isn't in the future (allowing for clock disagreement).
        && block_info_proposed.timestamp <= now + block_info_validation.block_timestamp_window_seconds
        && block_info_proposed.l1_da_mode == block_info_validation.l1_da_mode
        && block_info_proposed.l2_gas_price_fri == block_info_validation.l2_gas_price_fri)
    {
        warn!("Invalid BlockInfo. local_timestamp={now}");
        return false;
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

    let l1_gas_price_fri = l1_gas_prices.base_fee_per_gas.wei_to_fri(eth_to_fri_rate);
    let l1_data_gas_price_fri = l1_gas_prices.blob_fee.wei_to_fri(eth_to_fri_rate);
    let l1_gas_price_fri_proposed =
        block_info_proposed.l1_gas_price_wei.wei_to_fri(block_info_proposed.eth_to_fri_rate);
    let l1_data_gas_price_fri_proposed =
        block_info_proposed.l1_data_gas_price_wei.wei_to_fri(block_info_proposed.eth_to_fri_rate);
    if !(within_margin(l1_gas_price_fri_proposed, l1_gas_price_fri, l1_gas_price_margin_percent)
        && within_margin(
            l1_data_gas_price_fri_proposed,
            l1_data_gas_price_fri,
            l1_gas_price_margin_percent,
        ))
    {
        warn!(
            %l1_gas_price_fri_proposed,
            %l1_gas_price_fri,
            %l1_data_gas_price_fri_proposed,
            %l1_data_gas_price_fri,
            %l1_gas_price_margin_percent,
            "Invalid L1 gas price proposed.",
        );
        return false;
    }
    if l1_gas_price_fri_proposed != l1_gas_price_fri {
        CONSENSUS_L1_GAS_MISMATCH.increment(1);
    }
    if l1_data_gas_price_fri_proposed != l1_data_gas_price_fri {
        CONSENSUS_L1_DATA_GAS_MISMATCH.increment(1);
    }
    true
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
    fin_sender: oneshot::Sender<ProposalCommitment>,
    time: &dyn Clock,
) -> Option<(ConsensusBlockInfo, oneshot::Sender<ProposalCommitment>)> {
    tokio::select! {
        _ = cancel_token.cancelled() => {
            warn!("Proposal interrupted");
            None
        }
        _ = sleep_until(deadline, time) => {
            warn!("Validation timed out.");
            None
        }
        proposal_part = content_receiver.next() => {
            match proposal_part {
                Some(ProposalPart::BlockInfo(block_info)) => {
                    Some((block_info, fin_sender))
                }
                Some(ProposalPart::Fin(ProposalFin { proposal_commitment })) => {
                    warn!("Received an empty proposal.");
                    if fin_sender
                        .send(proposal_commitment)
                        .is_err()
                    {
                        // Consensus may exit early (e.g. sync).
                        warn!("Failed to send proposal content ids");
                    }
                    None
                }
                x => {
                    warn!("Invalid second proposal part: {x:?}");
                    None
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
    time: &dyn Clock,
) -> ProposalResult<()> {
    let chrono_timeout = chrono::Duration::from_std(timeout_plus_margin)
        .expect("Can't convert timeout to chrono::Duration");

    let input = ValidateBlockInput {
        proposal_id,
        deadline: time.now() + chrono_timeout,
        retrospective_block_hash: retrospective_block_hash(state_sync_client, &block_info).await?,
        block_info: convert_to_sn_api_block_info(&block_info),
    };
    debug!("Initiating validate proposal: input={input:?}");
    batcher.validate_block(input).await?;
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
    transaction_converter: Arc<dyn TransactionConverterTrait>,
) -> HandledProposalPart {
    match proposal_part {
        None => HandledProposalPart::Failed("Failed to receive proposal content".to_string()),
        Some(ProposalPart::Fin(fin)) => {
            info!("Received fin={fin:?}");
            // Output this along with the ID from batcher, to compare them.
            let input =
                SendProposalContentInput { proposal_id, content: SendProposalContent::Finish };
            let response = batcher.send_proposal_content(input).await.unwrap_or_else(|e| {
                panic!("Failed to send Fin to batcher: {proposal_id:?}. {e:?}")
            });
            let response_id = match response.response {
                ProposalStatus::Finished(id) => id,
                ProposalStatus::InvalidProposal => return HandledProposalPart::Invalid,
                status => panic!("Unexpected status: for {proposal_id:?}, {status:?}"),
            };
            let batcher_block_id = BlockHash(response_id.state_diff_commitment.0.0);
            let num_txs: usize = content.iter().map(|batch| batch.len()).sum();
            info!(
                network_block_id = ?fin.proposal_commitment,
                ?batcher_block_id,
                num_txs,
                "Finished validating proposal."
            );
            if num_txs == 0 {
                warn!("Validated an empty proposal.");
            }
            HandledProposalPart::Finished(batcher_block_id, fin)
        }
        Some(ProposalPart::Transactions(TransactionBatch { transactions: txs })) => {
            debug!("Received transaction batch with {} txs", txs.len());
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

fn convert_to_sn_api_block_info(block_info: &ConsensusBlockInfo) -> starknet_api::block::BlockInfo {
    let l1_gas_price_fri =
        NonzeroGasPrice::new(block_info.l1_gas_price_wei.wei_to_fri(block_info.eth_to_fri_rate))
            .unwrap();
    let l1_data_gas_price_fri = NonzeroGasPrice::new(
        block_info.l1_data_gas_price_wei.wei_to_fri(block_info.eth_to_fri_rate),
    )
    .unwrap();
    let l2_gas_price_fri = NonzeroGasPrice::new(block_info.l2_gas_price_fri).unwrap();
    let l2_gas_price_wei =
        NonzeroGasPrice::new(block_info.l2_gas_price_fri.fri_to_wei(block_info.eth_to_fri_rate))
            .unwrap();
    let l1_gas_price_wei = NonzeroGasPrice::new(block_info.l1_gas_price_wei).unwrap();
    let l1_data_gas_price_wei = NonzeroGasPrice::new(block_info.l1_data_gas_price_wei).unwrap();

    starknet_api::block::BlockInfo {
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
    }
}

async fn retrospective_block_hash(
    state_sync_client: Arc<dyn StateSyncClient>,
    block_info: &ConsensusBlockInfo,
) -> ProposalResult<Option<BlockHashAndNumber>> {
    let retrospective_block_number = block_info.height.0.checked_sub(STORED_BLOCK_HASH_BUFFER);
    let retrospective_block_hash = match retrospective_block_number {
        Some(block_number) => {
            let block_number = BlockNumber(block_number);
            let block = state_sync_client
                // Getting the next block hash because the Sync block only contains parent hash.
                .get_block(block_number.unchecked_next())
                .await?
                .ok_or(BuildProposalError::StateSyncNotReady(format!(
                "Failed to get retrospective block number {block_number}"
            )))?;
            Some(BlockHashAndNumber {
                number: block_number,
                hash: block.block_header_without_hash.parent_hash,
            })
        }
        None => {
            info!(
                "Retrospective block number is less than {STORED_BLOCK_HASH_BUFFER}, setting None \
                 as expected."
            );
            None
        }
    };
    Ok(retrospective_block_hash)
}
