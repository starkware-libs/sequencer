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
    FinishedProposalInfo,
    ProposalId,
    StartHeightInput,
};
use apollo_batcher_types::communication::{BatcherClient, BatcherClientError, BatcherClientResult};
use apollo_batcher_types::errors::BatcherError;
use apollo_config::behavior_mode::BehaviorMode;
use apollo_config_manager_types::communication::SharedConfigManagerClient;
use apollo_consensus::types::{ConsensusContext, ConsensusError, ProposalCommitment, Round};
use apollo_consensus_orchestrator_config::config::ContextConfig;
use apollo_l1_gas_price_types::L1GasPriceProviderClient;
use apollo_network::network_manager::{BroadcastTopicClient, BroadcastTopicClientTrait};
use apollo_protobuf::consensus::{
    BuildParam,
    CommitmentParts,
    HeightAndRound,
    L2GasInfo,
    ProposalFin,
    ProposalFinPayload,
    ProposalInit,
    ProposalPart,
    TransactionBatch,
    Vote,
};
use apollo_state_sync_types::communication::{
    SharedStateSyncClient,
    StateSyncClientError,
    StateSyncClientResult,
};
use apollo_state_sync_types::errors::StateSyncError;
use apollo_state_sync_types::state_sync_types::SyncBlock;
use apollo_time::time::Clock;
use apollo_transaction_converter::transaction_converter::{
    TransactionConverterError,
    TransactionConverterTrait,
};
use apollo_versioned_constants::VersionedConstants;
use async_trait::async_trait;
use futures::channel::mpsc::SendError;
use futures::channel::{mpsc, oneshot};
use futures::SinkExt;
use starknet_api::block::{
    BlockHashAndNumber,
    BlockHeaderWithoutHash,
    BlockInfo,
    BlockNumber,
    BlockTimestamp,
    GasPrice,
    StarknetVersion,
};
use starknet_api::block_hash::block_hash_calculator::BlockHeaderCommitments;
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::core::SequencerContractAddress;
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::execution_resources::GasAmount;
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::TransactionHash;
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;
use starknet_committer::patricia_merkle_tree::types::{
    CompressedPayload,
    CompressedStateCommitmentInfos,
    STATE_COMMITMENT_INFOS_VERSION,
};
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tokio_util::task::AbortOnDropHandle;
use tracing::{debug, error, error_span, info, instrument, trace, warn, Instrument};

use crate::build_proposal::{build_proposal, BuildProposalError, ProposalBuildArguments};
use crate::cende::{
    BlobParameters,
    CendeAmbassadorResult,
    CendeContext,
    InternalTransactionWithReceipt,
    StateCommitmentInfosAndNumber,
    N_BLOCK_HASHES_BACK_IN_BLOB,
};
use crate::dynamic_gas_price::{
    compute_fee_actual,
    compute_fee_proposal,
    compute_fee_target,
    proposal_commitment_from,
    FeeProposalInfo,
};
use crate::fee_market::{
    calculate_next_l2_gas_price_for_fin,
    get_min_gas_price_for_height,
    l2_gas_price_cap_for_height,
    FeeMarketInfo,
    NextL2GasPrice,
};
use crate::metrics::{
    record_build_proposal_failure,
    record_validate_proposal_failure,
    register_metrics,
    CONSENSUS_L2_GAS_PRICE,
    CONSENSUS_L2_GAS_PRICE_AT_MINIMUM,
    SNIP35_FEE_ACTUAL_FRI,
    SNIP35_FEE_PROPOSAL_FRI,
    SNIP35_FEE_TARGET_ABOVE_MAXIMUM,
    SNIP35_FEE_TARGET_ATTO_USD,
    SNIP35_FEE_TARGET_FRI,
};
use crate::utils::{
    convert_to_sn_api_block_info,
    make_gas_price_params,
    L1PricesInFri,
    L1PricesInWei,
    PreviousProposalInitInfo,
    StreamSender,
};
use crate::validate_proposal::{
    validate_proposal,
    ProposalInitValidation,
    ProposalValidateArguments,
    ValidateProposalError,
};

/// Maximum number of commitment infos backfilled to the cende recorder per blob.
const MAX_COMMITMENT_INFOS_BACKFILL: usize = 10;

/// First Starknet version whose blocks have state commitment infos.
const FIRST_VERSION_WITH_STATE_COMMITMENT_INFOS: StarknetVersion = StarknetVersion::V0_14_4;

type ValidationParams = (ProposalInit, Duration, mpsc::Receiver<ProposalPart>);

type ProposalContent =
    (ProposalInit, Vec<Vec<InternalConsensusTransaction>>, ProposalId, FinishedProposalInfo);

/// Assumption: at most one proposal is tracked per (height, round).
/// We still keep the `ProposalCommitment` alongside the content for:
/// - verifying `decision_reached(height, round, commitment)` matches what we stored, and
/// - supporting `repropose(proposal_commitment, init)` where `init.round` may differ from the round
///   the proposal was originally built/validated in.
type HeightToRoundToProposal =
    BTreeMap<BlockNumber, BTreeMap<Round, (ProposalCommitment, ProposalContent)>>;

pub(crate) struct BuiltProposals {
    // {height: {proposal_commitment: (init, content, proposal_id, finished_info)}}
    // Note that multiple proposals IDs can be associated with the same content, but we only need
    // to store one of them.
    //
    // The tranasactions are stored as a vector of batches (as returned from the batcher) and not
    // flattened. This is since we might need to repropose, in which case we need to send the
    // transactions in batches.
    data: HeightToRoundToProposal,
}

impl BuiltProposals {
    pub fn new() -> Self {
        Self { data: HeightToRoundToProposal::default() }
    }

    fn get_proposal(
        &self,
        height: &BlockNumber,
        round: &Round,
        commitment: &ProposalCommitment,
    ) -> &ProposalContent {
        let by_round = self
            .data
            .get(height)
            .unwrap_or_else(|| panic!("No proposals found for height {height}"));
        let (stored_commitment, stored_content) = by_round
            .get(round)
            .unwrap_or_else(|| panic!("No proposal found for height {height} and round {round}"));
        assert_eq!(
            stored_commitment, commitment,
            "Proposal commitment mismatch for height {height} round {round}: \
             stored={stored_commitment}, requested={commitment}"
        );
        stored_content
    }

    fn remove_proposals_below_or_at_height(&mut self, height: &BlockNumber) {
        self.data.retain(|&h, _| h > *height);
    }

    pub(crate) fn insert_proposal(
        &mut self,
        init: ProposalInit,
        transactions: Vec<Vec<InternalConsensusTransaction>>,
        proposal_id: &ProposalId,
        finished_info: FinishedProposalInfo,
    ) {
        let proposal_commitment = proposal_commitment_from(
            finished_info.proposal_commitment.partial_block_hash,
            init.fee_proposal_fri,
        );

        let height = init.height;
        let round = init.round;
        let by_round = self.data.entry(height).or_default();
        let previous = by_round.insert(
            round,
            (proposal_commitment, (init, transactions, *proposal_id, finished_info)),
        );
        assert!(
            previous.is_none(),
            "Overwriting existing proposal for height {height} round {round}; at most one \
             proposal per (height, round) is allowed"
        );
    }

    /// Gets the proposal from (height, valid_round), creates updated init with build_param for
    /// reproposal (timestamp remains unchanged since it is part of PartialBlockHashComponents and
    /// must match the initial proposal for commitment consistency), inserts a new entry at
    /// (height, build_param.round) so decision_reached can find it, and returns (init,
    /// transactions) for sending the reproposal.
    fn update_for_reproposal(
        &mut self,
        height: &BlockNumber,
        proposal_commitment: &ProposalCommitment,
        build_param: &BuildParam,
    ) -> (ProposalInit, Vec<Vec<InternalConsensusTransaction>>, FinishedProposalInfo) {
        let lookup_round = build_param.valid_round.expect("Valid round must be set for reproposal");
        let (mut init, transactions, proposal_id, finished_info) =
            self.get_proposal(height, &lookup_round, proposal_commitment).clone();
        init.round = build_param.round;
        init.proposer = build_param.proposer;
        init.valid_round = build_param.valid_round;
        self.insert_proposal(
            init.clone(),
            transactions.clone(),
            &proposal_id,
            finished_info.clone(),
        );
        (init, transactions, finished_info)
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
    previous_proposal_init: Option<PreviousProposalInitInfo>,
    /// fee_proposals window. `None` = pre-V0_14_3.
    fee_proposals_window: BTreeMap<BlockNumber, Option<GasPrice>>,
}

#[derive(Clone)]
pub struct SequencerConsensusContextDeps {
    pub transaction_converter: Arc<dyn TransactionConverterTrait>,
    pub state_sync_client: SharedStateSyncClient,
    pub batcher: Arc<dyn BatcherClient>,
    pub cende_ambassador: Arc<dyn CendeContext>,
    pub l1_gas_price_provider: Arc<dyn L1GasPriceProviderClient>,
    /// Use DefaultClock if you don't want to inject timestamps.
    pub clock: Arc<dyn Clock>,
    // Used to initiate new outbound proposal streams.
    pub outbound_proposal_sender: mpsc::Sender<(HeightAndRound, mpsc::Receiver<ProposalPart>)>,
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
            previous_proposal_init: None,
            fee_proposals_window: BTreeMap::new(),
        }
    }

    fn record_fee_proposal(&mut self, height: BlockNumber, fee_proposal_fri: Option<GasPrice>) {
        self.fee_proposals_window.insert(height, fee_proposal_fri);
    }

    fn prune_fee_proposals_window(&mut self, current_height: BlockNumber) {
        let window_size = VersionedConstants::latest_constants().fee_proposal_window_size;
        let cutoff = BlockNumber(current_height.0.saturating_sub(window_size));
        // Per `BTreeMap::split_off` docs: "Splits the collection into two at the given key.
        // Returns everything after the given key, including the key." Reassigning the returned
        // half back keeps `[cutoff, ..)` and drops everything below.
        self.fee_proposals_window = self.fee_proposals_window.split_off(&cutoff);
    }

    /// Fill `fee_proposals_window` from `[start_height - fee_proposal_window_size, start_height)`
    /// and `previous_proposal_init` from `start_height - 1`, both read from local state_sync
    /// storage. Must be called before `run_consensus`: joining consensus with a partial window
    /// would make this node disagree with caught-up peers on `fee_actual`, and joining with
    /// `previous_proposal_init` unset would make it propose an eth to fri rate its peers clamp.
    /// Waits for state_sync to reach each height.
    ///
    /// `previous_proposal_init` is also `try_sync`'s lower bound on a synced block's timestamp, so
    /// seeding it holds the first synced block after a restart to the same monotonicity every
    /// later one already faced.
    pub async fn initialize_from_committed_blocks(
        &mut self,
        start_height: BlockNumber,
    ) -> StateSyncClientResult<()> {
        let window_size = VersionedConstants::latest_constants().fee_proposal_window_size;
        let window_start_height = start_height.0.saturating_sub(window_size);
        for block_number in (window_start_height..start_height.0).map(BlockNumber) {
            let block_header = self.committed_block_header(block_number).await?;
            self.record_fee_proposal(block_number, block_header.fee_proposal_fri);
        }
        // Genesis has no previous block, so `previous_proposal_init` stays `None` and the first
        // block's eth to fri rate is unclamped and its timestamp unbounded from below.
        if let Some(previous_height) = start_height.prev() {
            let block_header = self.committed_block_header(previous_height).await?;
            self.previous_proposal_init =
                Some(previous_proposal_init_from_block_header(&block_header));
        }
        Ok(())
    }

    /// Reads a committed block's header from local state_sync storage, waiting for state_sync to
    /// catch up to the height. Other state_sync errors propagate.
    async fn committed_block_header(
        &self,
        block_number: BlockNumber,
    ) -> StateSyncClientResult<BlockHeaderWithoutHash> {
        const STATE_SYNC_RETRY_INTERVAL: Duration = Duration::from_millis(500);
        loop {
            match self.deps.state_sync_client.get_block(block_number).await {
                Ok(block) => return Ok(block.block_header_without_hash),
                Err(StateSyncClientError::StateSyncError(StateSyncError::BlockNotFound(_))) => {
                    warn!(
                        "State sync not ready for height {block_number}; retrying after \
                         {STATE_SYNC_RETRY_INTERVAL:?}"
                    );
                    tokio::time::sleep(STATE_SYNC_RETRY_INTERVAL).await;
                }
                Err(error) => return Err(error),
            }
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
            fee_proposal_fri: init.fee_proposal_fri,
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

    /// Returns the next L2 gas price without mutating context. Used when building the fin and when
    /// updating at decision time.
    fn calculate_next_l2_gas_price(
        &self,
        height: BlockNumber,
        l2_gas_used: GasAmount,
    ) -> NextL2GasPrice {
        let fee_actual = compute_fee_actual(
            &self.fee_proposals_window,
            height,
            VersionedConstants::latest_constants().fee_proposal_window_size,
        );
        calculate_next_l2_gas_price_for_fin(
            self.l2_gas_price,
            height,
            l2_gas_used,
            self.config.dynamic_config.override_l2_gas_price_fri,
            &self.config.dynamic_config.min_l2_gas_price_per_height,
            fee_actual,
        )
    }

    async fn resolve_fee_target(
        &self,
        timestamp: u64,
        target_atto_usd_per_l2_gas: u128,
    ) -> Option<GasPrice> {
        if let Some(v) = self.config.dynamic_config.override_l2_gas_price_fri {
            SNIP35_FEE_TARGET_FRI.set_lossy(v);
            return Some(GasPrice(v));
        }
        match self.deps.l1_gas_price_provider.get_strk_to_usd_rate(timestamp).await {
            Ok(rate) => {
                let target = compute_fee_target(target_atto_usd_per_l2_gas, rate);
                match target {
                    Some(t) => SNIP35_FEE_TARGET_FRI.set_lossy(t.0),
                    None => warn!("STRK/USD oracle returned zero rate, freezing fee_proposal"),
                }
                target
            }
            Err(e) => {
                warn!("STRK/USD oracle error: {e:?}, freezing fee_proposal");
                None
            }
        }
    }

    /// Compute the proposer's fee_proposal: clamp the oracle's `fee_target` to a margin around
    /// `fee_actual`, capped at the L2 gas price ceiling for `height`. When `fee_actual` is `None`
    /// (window incomplete), freeze at `l2_gas_price`, uncapped: the validator skips the band check
    /// without `fee_actual`, so both sides agree on the fallback, and the price users pay is capped
    /// in `calculate_next_l2_gas_price_for_fin`.
    async fn compute_proposer_fee_proposal(
        &self,
        height: BlockNumber,
        fee_actual: Option<GasPrice>,
        timestamp: u64,
        target_atto_usd_per_l2_gas: u128,
    ) -> GasPrice {
        SNIP35_FEE_TARGET_ATTO_USD.set_lossy(target_atto_usd_per_l2_gas);
        let Some(fee_actual) = fee_actual else {
            warn!("fee_actual unavailable, freezing fee_proposal at l2_gas_price");
            SNIP35_FEE_PROPOSAL_FRI.set_lossy(self.l2_gas_price.0);
            return self.l2_gas_price;
        };
        SNIP35_FEE_ACTUAL_FRI.set_lossy(fee_actual.0);

        let fee_target = self.resolve_fee_target(timestamp, target_atto_usd_per_l2_gas).await;
        let max_l2_gas_price = l2_gas_price_cap_for_height(
            height,
            &self.config.dynamic_config.min_l2_gas_price_per_height,
        );
        // `resolve_fee_target` returns the operator pin when `override_l2_gas_price_fri` is set;
        // a pin above the ceiling is deliberate, not oracle drift.
        let fee_target_is_from_oracle =
            self.config.dynamic_config.override_l2_gas_price_fri.is_none();
        let oracle_target_above_maximum =
            fee_target.filter(|target| fee_target_is_from_oracle && *target > max_l2_gas_price);
        if let Some(oracle_target) = oracle_target_above_maximum {
            warn!(
                "Oracle-derived fee_target {} exceeds the L2 gas price maximum {}, capping the \
                 fee_proposal band at the maximum",
                oracle_target.0, max_l2_gas_price.0
            );
            SNIP35_FEE_TARGET_ABOVE_MAXIMUM.increment(1);
        }

        let proposal = compute_fee_proposal(
            fee_target,
            fee_actual,
            VersionedConstants::latest_constants().fee_proposal_margin_ppt,
            max_l2_gas_price,
        );
        SNIP35_FEE_PROPOSAL_FRI.set_lossy(proposal.0);
        proposal
    }

    /// The values a proposal for `height` is validated against, all derived from this node's own
    /// state and config, never from the proposal. Both validation paths, the current round and a
    /// proposal dequeued on a round change, build it here so they judge the same proposal alike.
    fn proposal_init_validation(&self, height: BlockNumber) -> ProposalInitValidation {
        ProposalInitValidation {
            height,
            block_timestamp_window_seconds: self
                .config
                .static_config
                .block_timestamp_window_seconds,
            previous_proposal_init: self.previous_proposal_init.clone(),
            l1_da_mode: self.l1_da_mode,
            l2_gas_price_fri: self
                .config
                .dynamic_config
                .override_l2_gas_price_fri
                .map(GasPrice)
                .unwrap_or(self.l2_gas_price),
            starknet_version: StarknetVersion::LATEST,
            fee_actual: compute_fee_actual(
                &self.fee_proposals_window,
                height,
                VersionedConstants::latest_constants().fee_proposal_window_size,
            ),
            max_l2_gas_price: l2_gas_price_cap_for_height(
                height,
                &self.config.dynamic_config.min_l2_gas_price_per_height,
            ),
        }
    }

    fn update_l2_gas_price(&mut self, height: BlockNumber, l2_gas_used: GasAmount) {
        let next_l2_gas_price = self.calculate_next_l2_gas_price(height, l2_gas_used);
        // Only this path runs once per decided block, so the clamp counter is reported here rather
        // than inside the shared computation.
        next_l2_gas_price.record_clamping();
        self.l2_gas_price = next_l2_gas_price.published_price;
        let gas_price_u64 = u64::try_from(self.l2_gas_price.0).unwrap_or(u64::MAX);
        CONSENSUS_L2_GAS_PRICE.set_lossy(gas_price_u64);
        let config_min = get_min_gas_price_for_height(
            height,
            &self.config.dynamic_config.min_l2_gas_price_per_height,
        );
        CONSENSUS_L2_GAS_PRICE_AT_MINIMUM.set_lossy(u64::from(self.l2_gas_price <= config_min));
    }

    #[allow(clippy::too_many_arguments)]
    async fn finalize_decision(
        &mut self,
        height: BlockNumber,
        init: &ProposalInit,
        commitment: ProposalCommitment,
        // Accepts transactions as a vector of batches, as stored in the `BuiltProposals` map.
        transactions: Vec<Vec<InternalConsensusTransaction>>,
        decision_reached_response: DecisionReachedResponse,
        block_header_commitments: BlockHeaderCommitments,
        l2_gas_used: GasAmount,
        wait_for_last_commitment: bool,
    ) {
        let DecisionReachedResponse { state_diff, central_objects } = decision_reached_response;

        self.update_l2_gas_price(height, l2_gas_used);
        self.record_fee_proposal(height, init.fee_proposal_fri);

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
        let cende_block_info = convert_to_sn_api_block_info(init).expect(
            "Failed to convert block info to SN API block info (required for state sync and \
             preparing the cende blob). IMPORTANT: The block was committed; a revert might be \
             required for the node to be able to proceed.",
        );

        if let Err(e) = self
            .update_state_sync_with_new_block(
                height,
                &state_diff,
                &transactions_with_execution_infos,
                init,
                &cende_block_info,
                l2_gas_used,
                block_header_commitments.clone(),
            )
            .await
        {
            // TODO(Shahak): Decide how to handle this error once p2p state sync is
            // production-ready. At this point, the block has already been committed to
            // the state.
            warn!("Failed to update state sync with new block at height {height}: {e:?}");
        }

        // At stop height, block N's hash may not yet be available; wait before preparing the blob.
        if wait_for_last_commitment {
            self.wait_for_block_hash(height).await;
            self.warn_on_missing_state_commitment_infos_at_stop_height(height).await;
        }

        // The parent block's `fee_proposal_fri` is needed to reconstruct its V0_14_3 commitment
        // (`Poseidon(partial_block_hash, fee_proposal_fri)`). It is read from the in-memory
        // `fee_proposals_window`, which mirrors `BlockHeaderWithoutHash` storage. `None` means the
        // parent is pre-V0_14_3 (or, near genesis, is absent from the window).
        let parent_fee_proposal = height
            .prev()
            .and_then(|parent_height| self.fee_proposals_window.get(&parent_height).copied())
            .flatten();
        // Collected first: `get_block_hash` flushes finished committer results to storage,
        // making this height's state commitment infos readable in the collection below.
        let recent_block_hashes = self.collect_recent_block_hashes(height).await;
        let recent_state_commitment_infos =
            self.collect_recent_state_commitment_infos(height).await.unwrap_or_else(|e| {
                // `finalize_decision` must not fail, so continue with an empty vector.
                error!("Failed to collect recent state commitment infos at height {height}: {e:?}");
                Vec::new()
            });

        if let Err(e) = self
            .deps
            .cende_ambassador
            .prepare_blob_for_next_height(BlobParameters {
                block_info: cende_block_info,
                starknet_version: init.starknet_version,
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
                // Forward the proposer's stated fee_proposal_fri (from ProposalInit)
                // to the centralized cende pipeline. The centralized side persists this in
                // its own storage namespace, separate from FeeMarketInfo. Pre-V0_14_3 blocks
                // have `init.fee_proposal_fri == None`.
                fee_proposal_info: FeeProposalInfo { fee_proposal_fri: init.fee_proposal_fri },
                compiled_class_hashes_for_migration: central_objects
                    .compiled_class_hashes_for_migration,
                proposal_commitment: commitment,
                parent_proposal_commitment: central_objects
                    .parent_proposal_commitment
                    .map(|c| proposal_commitment_from(c.partial_block_hash, parent_fee_proposal)),
                recent_block_hashes,
                recent_state_commitment_infos,
                initial_reads: central_objects.initial_reads,
                block_hash_commitments: block_header_commitments,
            })
            .await
        {
            error!("Failed to prepare blob for next height at height {height}: {e:?}");
        }
    }

    pub fn get_config(&self) -> &ContextConfig {
        &self.config
    }

    /// Waits until the batcher has computed the hash for `block_number`.
    /// Used at stop height to ensure block N's hash is included in the blob before it is written.
    async fn wait_for_block_hash(&self, block_number: BlockNumber) {
        let retry_interval =
            self.config.static_config.retrospective_block_hash_retry_interval_millis;
        info!("Waiting for block hash of {block_number} before writing blob at stop height.");
        loop {
            match self.deps.batcher.get_block_hash(block_number).await {
                Ok(_) => {
                    info!("Block hash of {block_number} is available.");
                    return;
                }
                Err(BatcherClientError::BatcherError(BatcherError::BlockHashNotFound(_))) => {
                    info!("Block hash of {block_number} not yet available, retrying.");
                }
                Err(err) => {
                    warn!(
                        "Unexpected error while waiting for block hash of {block_number}: {err:?}"
                    );
                }
            }
            sleep(retry_interval).await;
        }
    }

    /// Collects the recent block hashes from the batcher.
    /// Returns computed block hashes in range [height - N_BLOCK_HASHES_BACK_IN_BLOB, height].
    async fn collect_recent_block_hashes(&self, height: BlockNumber) -> Vec<BlockHashAndNumber> {
        let mut recent_block_hashes = Vec::with_capacity(
            usize::try_from(N_BLOCK_HASHES_BACK_IN_BLOB)
                .expect("N_BLOCK_HASHES_BACK_IN_BLOB should fit in usize.")
                + 1,
        );
        let lowest_height = height.0.saturating_sub(N_BLOCK_HASHES_BACK_IN_BLOB);
        for height in lowest_height..=height.0 {
            let block_number = BlockNumber(height);
            match self.deps.batcher.get_block_hash(block_number).await {
                Ok(block_hash) => {
                    recent_block_hashes
                        .push(BlockHashAndNumber { number: block_number, hash: block_hash });
                }
                Err(err) => {
                    // This error is expected if the block is not yet committed.
                    if !matches!(
                        err,
                        BatcherClientError::BatcherError(BatcherError::BlockHashNotFound(_))
                    ) {
                        // TODO(Nimrod): Consider handle this error differently.
                        warn!("Failed to get block hash from batcher: {err:?}");
                    }
                    break;
                }
            }
        }
        recent_block_hashes
    }

    async fn collect_recent_state_commitment_infos(
        &self,
        height: BlockNumber,
    ) -> CendeAmbassadorResult<Vec<StateCommitmentInfosAndNumber>> {
        // Send only the commitment infos the cende recorder has not stored yet, bounding the
        // lower end by the cende recorder's commitment infos height offset.
        let (lowest_height, cende_recorder_is_empty) =
            match self.deps.cende_ambassador.commitment_infos_height_offset().await? {
                Some(commitment_infos_height_offset) => (commitment_infos_height_offset.0, false),
                // The cende recorder has stored nothing yet: fall back to block hashes window
                // size.
                None => (height.0.saturating_sub(N_BLOCK_HASHES_BACK_IN_BLOB), true),
            };
        let mut recent_state_commitment_infos = Vec::with_capacity(MAX_COMMITMENT_INFOS_BACKFILL);
        for block_height in lowest_height..=height.0 {
            // Stop at the cap, so sending commitment infos won't stall block production.
            if recent_state_commitment_infos.len() >= MAX_COMMITMENT_INFOS_BACKFILL {
                break;
            }
            let block_number = BlockNumber(block_height);
            match self.state_commitment_infos_to_send(block_number).await {
                Ok(Some(state_commitment_infos)) => recent_state_commitment_infos
                    .push(StateCommitmentInfosAndNumber { state_commitment_infos, block_number }),
                Ok(None) => {
                    // Only an empty cende recorder's fallback window can reach back before
                    // FIRST_VERSION_WITH_STATE_COMMITMENT_INFOS; a block that old has no
                    // commitment infos to miss.
                    if cende_recorder_is_empty
                        && self.predates_state_commitment_infos(block_number).await
                    {
                        debug!(
                            "Block {block_number} predates \
                             {FIRST_VERSION_WITH_STATE_COMMITMENT_INFOS}; skipping its state \
                             commitment infos."
                        );
                        continue;
                    }
                    // Break, rather than continue, in both cases that reach here:
                    // - Normal production: the cende recorder is not empty, so this is the first
                    //   block past the latest one with state commitment infos.
                    // - The cende recorder is empty and, by the condition above, the block is not
                    //   pre-0.14.4; it is the system's first block with state commitment infos,
                    //   left for the next blob. Continuing would send a later height's commitment
                    //   infos and leave a gap here.
                    break;
                }
                Err(err) => {
                    error!(
                        "Failed to get state commitment infos from batcher for block \
                         {block_number}: {err:?}"
                    );
                    break;
                }
            }
        }
        Ok(recent_state_commitment_infos)
    }

    /// Returns whether `block_number` predates `FIRST_VERSION_WITH_STATE_COMMITMENT_INFOS`, read
    /// from its header once the block reaches state sync.
    async fn predates_state_commitment_infos(&self, block_number: BlockNumber) -> bool {
        match self.deps.state_sync_client.get_block(block_number).await {
            Ok(sync_block) => {
                sync_block.block_header_without_hash.starknet_version
                    < FIRST_VERSION_WITH_STATE_COMMITMENT_INFOS
            }
            Err(err) => {
                warn!(
                    "Failed to get the Starknet version of block {block_number} from state sync: \
                     {err:?}"
                );
                false
            }
        }
    }

    /// Returns the state commitment infos of `block_number` to send to the cende recorder, or
    /// `None` if the batcher has none. With `send_empty_state_commitment_infos_only` on, an empty
    /// object stands in for the stored one, which is not read.
    async fn state_commitment_infos_to_send(
        &self,
        block_number: BlockNumber,
    ) -> BatcherClientResult<Option<CompressedStateCommitmentInfos>> {
        if !self.config.static_config.send_empty_state_commitment_infos_only {
            return self.deps.batcher.get_state_commitment_infos(block_number).await;
        }
        let has_state_commitment_infos =
            self.deps.batcher.has_state_commitment_infos(block_number).await?;
        Ok(has_state_commitment_infos.then(|| CompressedStateCommitmentInfos {
            version: STATE_COMMITMENT_INFOS_VERSION,
            payload: CompressedPayload(Vec::new()),
        }))
    }

    /// Checks at the stop height, once its block hash is available, that the batcher has stored
    /// the stop height's state commitment infos.
    async fn warn_on_missing_state_commitment_infos_at_stop_height(
        &self,
        stop_height: BlockNumber,
    ) {
        match self.deps.batcher.has_state_commitment_infos(stop_height).await {
            Ok(true) => {
                info!(
                    "The batcher has the state commitment infos of the stop height {stop_height}."
                );
            }
            Ok(false) => {
                warn!(
                    "Stopping at height {stop_height}, but the batcher has no state commitment \
                     infos for it; the final blob will not carry them."
                );
            }
            Err(err) => {
                warn!(
                    "Failed to check whether the batcher has the state commitment infos of the \
                     stop height {stop_height}: {err:?}. Cannot verify the final blob will carry \
                     them."
                );
            }
        }
    }
}

#[async_trait]
impl ConsensusContext for SequencerConsensusContext {
    type ProposalPart = ProposalPart;

    #[instrument(skip_all)]
    async fn build_proposal(
        &mut self,
        build_param: BuildParam,
        timeout: Duration,
    ) -> Result<oneshot::Receiver<ProposalCommitment>, ConsensusError> {
        let cende_write_success = AbortOnDropHandle::new(
            self.deps.cende_ambassador.write_prev_height_blob(build_param.height),
        );

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
        assert!(timeout > self.config.dynamic_config.build_proposal_margin_millis);
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
            timeout - self.config.dynamic_config.build_proposal_margin_millis;
        let time_now = self.deps.clock.now();
        let batcher_deadline = time_now + total_build_proposal_time;
        let retrospective_block_hash_deadline = time_now
            + total_build_proposal_time.mul_f32(
                self.config.static_config.build_proposal_time_ratio_for_retrospective_block_hash,
            );

        let override_block_metadata = match self.config.static_config.behavior_mode {
            BehaviorMode::Echonet => true,
            BehaviorMode::Starknet => false,
        };
        let fee_actual = compute_fee_actual(
            &self.fee_proposals_window,
            build_param.height,
            VersionedConstants::latest_constants().fee_proposal_window_size,
        );
        let fee_proposal = self
            .compute_proposer_fee_proposal(
                build_param.height,
                fee_actual,
                self.deps.clock.unix_now(),
                self.config.dynamic_config.snip35_target_atto_usd_per_l2_gas,
            )
            .await;
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
            previous_proposal_init: self.previous_proposal_init.clone(),
            proposal_round: self.current_round,
            retrospective_block_hash_deadline,
            retrospective_block_hash_retry_interval_millis: self
                .config
                .static_config
                .retrospective_block_hash_retry_interval_millis,
            override_block_metadata,
            override_l2_gas_price_fri: self.config.dynamic_config.override_l2_gas_price_fri,
            min_l2_gas_price_per_height: self
                .config
                .dynamic_config
                .min_l2_gas_price_per_height
                .clone(),
            compare_retrospective_block_hash: self
                .config
                .dynamic_config
                .compare_retrospective_block_hash,
            fee_proposal,
            fee_actual,
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
        content_receiver: mpsc::Receiver<Self::ProposalPart>,
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
                let proposal_init_validation = self.proposal_init_validation(init.height);
                self.validate_current_round_proposal(
                    init,
                    proposal_init_validation,
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

    async fn repropose(
        &mut self,
        proposal_commitment: ProposalCommitment,
        build_param: BuildParam,
    ) {
        info!(?proposal_commitment, ?build_param, "Reproposing.");
        let height = build_param.height;
        let round = build_param.round;
        // Tear down any proposal still tracked from a previous round before starting this one,
        // mirroring build/validate. set_height_and_round already interrupts on round change; we
        // repeat it here so the active-proposal slot is guaranteed free without asserting.
        self.interrupt_active_proposal().await;

        let (init, txs, finished_info) = self
            .valid_proposals
            .lock()
            .expect("Lock on active proposals was poisoned due to a previous panic")
            .update_for_reproposal(&height, &proposal_commitment, &build_param);

        let next_l2_gas_price = self
            .calculate_next_l2_gas_price(init.height, finished_info.l2_gas_used)
            .published_price;
        let transaction_converter = self.deps.transaction_converter.clone();
        let mut stream_sender = self.start_stream(HeightAndRound(height.0, round)).await;

        // Track the reproposal task like build/validate so it is cancelled and awaited on
        // round/height change. Otherwise a superseded reproposal keeps re-converting every
        // transaction through the class manager and holding a full-block clone, with no
        // backpressure on the number of in-flight tasks.
        let cancel_token = CancellationToken::new();
        let cancel_token_clone = cancel_token.clone();
        let handle = tokio::spawn(
            async move {
                let res = tokio::select! {
                    biased;
                    () = cancel_token.cancelled() => {
                        info!(?height, round, "Reproposal cancelled for superseded round.");
                        return;
                    }
                    res = send_reproposal(
                        proposal_commitment,
                        init,
                        txs,
                        finished_info,
                        next_l2_gas_price,
                        &mut stream_sender,
                        transaction_converter,
                    ) => res,
                };
                match res {
                    Ok(()) => {
                        info!(?proposal_commitment, ?build_param, "Reproposal succeeded.");
                    }
                    Err(e) => {
                        warn!("REPROPOSE_FAILED: Reproposal failed. Error: {e:?}");
                    }
                }
            }
            .instrument(error_span!("consensus_repropose", round)),
        );
        // Panics in the task surface as a JoinError when interrupt_active_proposal awaits the
        // handle on the next round/height change or on decision_reached, matching build/validate.
        self.active_proposal = Some((cancel_token_clone, handle));
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
        round: Round,
        commitment: ProposalCommitment,
        wait_for_last_commitment: bool,
    ) -> Result<(), ConsensusError> {
        info!("Finished consensus for height: {height}. Agreed on block: {:#066x}", commitment.0);

        self.interrupt_active_proposal().await;
        let (init, transactions, proposal_id, finished_info) = {
            let mut proposals = self.valid_proposals.lock().unwrap();
            let (init, transactions, proposal_id, finished_info) =
                proposals.get_proposal(&height, &round, &commitment).clone();
            proposals.remove_proposals_below_or_at_height(&height);
            (init, transactions, proposal_id, finished_info)
        };

        let decision_reached_response =
            self.deps.batcher.decision_reached(DecisionReachedInput { proposal_id }).await?;

        // CRITICAL: The block is now committed. This function must not fail beyond this point
        // unless the state is fully reverted, otherwise the node will be left in an
        // inconsistent state.

        self.finalize_decision(
            height,
            &init,
            commitment,
            transactions,
            decision_reached_response,
            finished_info.block_header_commitments.clone(),
            finished_info.l2_gas_used,
            wait_for_last_commitment,
        )
        .await;

        // At stop height, immediately write the blob instead of waiting for the next proposal
        // build.
        if wait_for_last_commitment
            && !self
                .deps
                .cende_ambassador
                .write_prev_height_blob(height.unchecked_next())
                .await
                .unwrap_or(false)
        {
            error!("Failed to write blob at stop height {height}.");
        }

        self.previous_proposal_init = Some(PreviousProposalInitInfo::from(&init));

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
            self.previous_proposal_init.as_ref().map_or(0, |info| info.timestamp);
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
        // Fetch the block's proof facts and execution infos from the centralized recorder and
        // compute the accessed keys locally, so the batcher can build the block's state commitment
        // infos. Done before updating any consensus state so that a recorder failure leaves the
        // context unchanged for the retried sync attempt.
        let accessed_keys = if self.config.static_config.fetch_accessed_keys_from_centralized {
            match self.deps.cende_ambassador.get_accessed_keys_input(block_number).await {
                Ok(Some(block_data)) => {
                    info!("Fetched accessed-keys data for synced block {block_number}.");
                    Some(block_data.compute_accessed_keys(&sync_block.state_diff.clone().into()))
                }
                Ok(None) => {
                    error!(
                        "Recorder does not yet have accessed-keys data for synced block \
                         {block_number}."
                    );
                    return false;
                }
                Err(e) => {
                    error!(
                        "Failed to fetch accessed-keys data for synced block {block_number}: {e:?}"
                    );
                    return false;
                }
            }
        } else {
            None
        };

        self.record_fee_proposal(height, sync_block.block_header_without_hash.fee_proposal_fri);
        self.previous_proposal_init =
            Some(previous_proposal_init_from_block_header(&sync_block.block_header_without_hash));
        self.interrupt_active_proposal().await;

        info!(
            "Adding sync block to Batcher for height {}",
            sync_block.block_header_without_hash.block_number,
        );
        if let Err(e) = self.deps.batcher.add_sync_block(sync_block, accessed_keys).await {
            error!("Failed to add sync block to Batcher: {e:?}");
            return false;
        }

        true
    }

    async fn network_tip(&self) -> Option<BlockNumber> {
        // Fail-open: on error, report an unknown tip so consensus proceeds as usual. Logged at
        // debug (not error) because this is queried every height and a transient miss is benign;
        // a persistently unknown tip shows up as repeated lines worth investigating.
        self.deps.state_sync_client.get_highest_block_number().await.unwrap_or_else(|err| {
            debug!("Failed to fetch network tip from state sync: {err}");
            None
        })
    }

    async fn set_height_and_round(
        &mut self,
        height: BlockNumber,
        round: Round,
    ) -> Result<(), ConsensusError> {
        // First height or a new (higher) height.
        if self.current_height.is_none_or(|h| height > h) {
            self.update_dynamic_config().await;
            // On first height: initialize l2_gas_price to the configured minimum for this height,
            // ensuring correct startup after restart/revert.
            if self.current_height.is_none() {
                let min_gas_price_for_height = get_min_gas_price_for_height(
                    height,
                    &self.config.dynamic_config.min_l2_gas_price_per_height,
                );
                self.l2_gas_price = max(self.l2_gas_price, min_gas_price_for_height);
                let gas_price_u64 = u64::try_from(self.l2_gas_price.0).unwrap_or(u64::MAX);
                CONSENSUS_L2_GAS_PRICE.set_lossy(gas_price_u64);
                CONSENSUS_L2_GAS_PRICE_AT_MINIMUM
                    .set_lossy(u64::from(self.l2_gas_price <= min_gas_price_for_height));
            }
            self.current_height = Some(height);
            self.current_round = round;
            self.prune_fee_proposals_window(height);
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
                // The queued proposal is for a past round; drop it and keep scanning.
                std::cmp::Ordering::Greater => {
                    entry.remove();
                }
                // The queued proposal is for the current round; take it and stop.
                std::cmp::Ordering::Equal => {
                    to_process = Some(entry.remove());
                    break;
                }
                // The queued proposal is for a future round; preserve it for later.
                std::cmp::Ordering::Less => break,
            }
        }
        // Validate the proposal for the current round if exists.
        let Some(((init, timeout, content), fin_sender)) = to_process else {
            return Ok(());
        };
        let proposal_init_validation = self.proposal_init_validation(init.height);
        self.validate_current_round_proposal(
            init,
            proposal_init_validation,
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
        proposal_init_validation: ProposalInitValidation,
        timeout: Duration,
        batcher_timeout_margin: Duration,
        content_receiver: mpsc::Receiver<ProposalPart>,
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
            proposal_init_validation,
            proposal_id,
            timeout,
            batcher_timeout_margin,
            valid_proposals: Arc::clone(&self.valid_proposals),
            content_receiver,
            gas_price_params,
            cancel_token: cancel_token_clone,
            compare_retrospective_block_hash: self
                .config
                .dynamic_config
                .compare_retrospective_block_hash,
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
    proposal_commitment: ProposalCommitment,
    init: ProposalInit,
    txs: Vec<Vec<InternalConsensusTransaction>>,
    finished_info: FinishedProposalInfo,
    next_l2_gas_price: GasPrice,
    stream_sender: &mut StreamSender,
    transaction_converter: Arc<dyn TransactionConverterTrait>,
) -> Result<(), ReproposeError> {
    stream_sender.send(ProposalPart::Init(init)).await?;
    for batch in txs.into_iter() {
        let transactions = futures::future::join_all(batch.into_iter().map(|tx| {
            // transaction_converter is an external dependency (class manager) and so
            // we can't assume success on reproposal.
            transaction_converter.convert_internal_consensus_tx_to_consensus_tx(tx)
        }))
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
        stream_sender.send(ProposalPart::Transactions(TransactionBatch { transactions })).await?;
    }
    let executed_transaction_count: u64 = finished_info
        .final_n_executed_txs
        .try_into()
        .expect("Number of executed transactions should fit in u64");
    let fin_payload = ProposalFinPayload {
        commitment_parts: CommitmentParts::from(&finished_info),
        l2_gas_info: L2GasInfo {
            next_l2_gas_price_fri: next_l2_gas_price,
            l2_gas_used: finished_info.l2_gas_used,
        },
    };
    let fin = ProposalFin {
        proposal_commitment,
        executed_transaction_count,
        fin_payload: Some(fin_payload),
    };
    stream_sender.send(ProposalPart::Fin(fin)).await?;

    Ok(())
}

fn previous_proposal_init_from_block_header(
    block_header: &BlockHeaderWithoutHash,
) -> PreviousProposalInitInfo {
    PreviousProposalInitInfo {
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
