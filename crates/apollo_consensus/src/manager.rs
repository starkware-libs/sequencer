//! Top level of consensus, used to run multiple heights of consensus.
//!
//! [`run_consensus`] - This is the primary entrypoint for running the consensus component.
//!
//! [`MultiHeightManager`] - Runs consensus repeatedly across different heights using
//! [`run_height`](MultiHeightManager::run_height).

#[cfg(test)]
#[path = "manager_test.rs"]
mod manager_test;

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use apollo_config_manager_types::communication::SharedConfigManagerClient;
use apollo_consensus_config::config::{
    ConsensusConfig,
    ConsensusDynamicConfig,
    FutureMsgLimitsConfig,
};
use apollo_infra_utils::debug_every_n_ms;
use apollo_network::network_manager::BroadcastTopicClientTrait;
use apollo_network_types::network_types::{
    BadPeerReason,
    BadPeerReport,
    BroadcastedMessageMetadata,
    PenaltyCard,
};
use apollo_protobuf::consensus::{BuildParam, ProposalInit, Vote, VoteType};
use apollo_protobuf::converters::ProtobufConversionError;
use apollo_staking::committee_provider::{CommitteeProvider, CommitteeTrait};
use apollo_time::time::{Clock, ClockExt, DefaultClock};
use futures::channel::mpsc::{self};
use futures::future::BoxFuture;
use futures::stream::FuturesUnordered;
use futures::{FutureExt, StreamExt};
use starknet_api::block::BlockNumber;
use tokio::sync::Mutex;
use tracing::{debug, error, info, instrument, trace, warn};

use crate::metrics::{
    register_metrics,
    CONSENSUS_BLOCK_NUMBER,
    CONSENSUS_CACHED_VOTES,
    CONSENSUS_DECISIONS_REACHED_AS_PROPOSER,
    CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS,
    CONSENSUS_DECISIONS_REACHED_BY_SYNC,
    CONSENSUS_MAX_CACHED_BLOCK_NUMBER,
    CONSENSUS_PROPOSALS_RECEIVED,
    CONSENSUS_REPROPOSALS,
};
use crate::single_height_consensus::{Requests, SingleHeightConsensus};
use crate::state_machine::{SMRequest, StateMachineEvent, Step};
use crate::storage::HeightVotedStorageTrait;
use crate::types::{
    BroadcastVoteChannel,
    ConsensusContext,
    ConsensusError,
    Decision,
    Round,
    ValidatorId,
};
use crate::votes_threshold::QuorumType;

/// Arguments for running consensus.
pub struct RunConsensusArguments {
    /// Consensus configuration (static + dynamic). Static fields are used directly; dynamic
    /// fields are refreshed at height boundaries via `config_manager_client` when provided.
    pub consensus_config: ConsensusConfig,
    /// The height at which the node begins to run consensus.
    pub start_active_height: BlockNumber,
    /// Set to Byzantine by default. Using Honest means we trust all validators. Use with caution!
    pub quorum_type: QuorumType,
    /// Optional client for fetching dynamic consensus config between heights.
    pub config_manager_client: Option<SharedConfigManagerClient>,
    /// Storage used to persist last voted consensus height.
    // See MultiHeightManager foran explanation of why we have Arc<Mutex>>.
    pub last_voted_height_storage: Arc<Mutex<dyn HeightVotedStorageTrait>>,
    /// Provider for committee (validators, proposer).
    pub committee_provider: Arc<dyn CommitteeProvider>,
}

impl std::fmt::Debug for RunConsensusArguments {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunConsensusArguments")
            .field("start_active_height", &self.start_active_height)
            .field("dynamic_config", &self.consensus_config.dynamic_config)
            .field("static_config", &self.consensus_config.static_config)
            .field("quorum_type", &self.quorum_type)
            .field("last_voted_height_storage", &self.last_voted_height_storage)
            .finish()
    }
}

/// Run consensus indefinitely.
///
/// If a decision is reached via consensus, the context is updated. If a decision is learned via the
/// sync protocol, consensus silently moves on to the next height.
///
/// Inputs:
/// - `run_consensus_args`: Configuration arguments for consensus. See [`RunConsensusArguments`] for
///   detailed documentation.
/// - `context`: The API for consensus to reach out to the rest of the node.
/// - `vote_receiver`: The channels to receive votes from the network. These are self contained
///   messages.
/// - `proposals_receiver`: The channel to receive proposals from the network. Proposals are
///   represented as streams (ProposalInit, Content.*, ProposalFin).
// Always print the validator ID since some tests collate multiple consensus logs in a single file.
#[instrument(
    skip_all,
    fields(validator_id=%run_consensus_args.consensus_config.dynamic_config.validator_id),
    level = "error"
)]
pub async fn run_consensus<ContextT>(
    run_consensus_args: RunConsensusArguments,
    mut context: ContextT,
    mut vote_receiver: BroadcastVoteChannel,
    mut proposals_receiver: mpsc::Receiver<mpsc::Receiver<ContextT::ProposalPart>>,
) -> Result<(), ConsensusError>
where
    ContextT: ConsensusContext,
{
    info!("Running consensus, args: {:?}", run_consensus_args);
    register_metrics();
    // Add a short delay to allow peers to connect and avoid "InsufficientPeers" error
    tokio::time::sleep(run_consensus_args.consensus_config.static_config.startup_delay).await;

    let mut current_height = run_consensus_args.start_active_height;
    let mut manager = MultiHeightManager::new(
        run_consensus_args.consensus_config.clone(),
        run_consensus_args.quorum_type,
        run_consensus_args.last_voted_height_storage.clone(),
        run_consensus_args.committee_provider.clone(),
    )
    .await;
    loop {
        if let Some(client) = &run_consensus_args.config_manager_client {
            match client.get_consensus_dynamic_config().await {
                Ok(dynamic_cfg) => {
                    manager.set_dynamic_config(dynamic_cfg);
                }
                Err(e) => {
                    error!(
                        "get_consensus_dynamic_config failed: {e}. Using previous dynamic config."
                    );
                }
            }
        }

        match manager
            .run_height(&mut context, current_height, &mut vote_receiver, &mut proposals_receiver)
            .await?
        {
            RunHeightRes::Decision(decision) => {
                // We expect there to be under 100 validators, so this is a reasonable number of
                // precommits to print.
                let round = decision.precommits[0].round;
                // Proposer should never fail here, we already checked it in the state machine.
                let proposer = get_proposer_for_height(
                    &run_consensus_args.committee_provider,
                    current_height,
                    round,
                )
                .await?;

                if proposer == run_consensus_args.consensus_config.dynamic_config.validator_id {
                    CONSENSUS_DECISIONS_REACHED_AS_PROPOSER.increment(1);
                }
                info!(
                    "DECISION_REACHED: Decision reached for round {} with proposer {}. {:?}",
                    round, proposer, decision
                );
                CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS.increment(1);
            }
            RunHeightRes::Sync => {
                info!(height = current_height.0, "Decision learned via sync protocol.");
                CONSENSUS_DECISIONS_REACHED_BY_SYNC.increment(1);
            }
        }
        current_height = current_height.unchecked_next();
    }
}

/// Run height can end either when consensus reaches a decision or when we learn, via sync, of the
/// decision.
#[derive(Debug, PartialEq)]
pub enum RunHeightRes {
    /// Decision reached.
    Decision(Decision),
    /// Decision learned via sync.
    Sync,
}

type ProposalReceiverTuple<T> = (ProposalInit, mpsc::Receiver<T>);

// TODO(guyn): remove allow(dead_code) once we use this struct.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct EquivocationVoteReport {
    pub cached_vote: Vote,
    pub new_vote: Vote,
}

/// Manages votes and proposals for future heights.
#[derive(Debug)]
struct ConsensusCache<ContextT: ConsensusContext> {
    // Mapping: { Height : Vec<Vote> }
    future_votes: BTreeMap<BlockNumber, Vec<Vote>>,
    // Mapping: { Height : { Round : (BlockInfo, Receiver)}}
    future_proposals_cache:
        BTreeMap<BlockNumber, BTreeMap<Round, ProposalReceiverTuple<ContextT::ProposalPart>>>,
    /// Configuration for determining which messages should be cached.
    future_msg_limit: FutureMsgLimitsConfig,
}

impl<ContextT: ConsensusContext> ConsensusCache<ContextT> {
    fn new(future_msg_limit: FutureMsgLimitsConfig) -> Self {
        Self {
            future_votes: BTreeMap::new(),
            future_proposals_cache: BTreeMap::new(),
            future_msg_limit,
        }
    }

    /// Update the future message limits configuration.
    fn set_future_msg_limit(&mut self, future_msg_limit: FutureMsgLimitsConfig) {
        self.future_msg_limit = future_msg_limit;
    }

    /// Filters the cached messages:
    /// - returns (and removes from stored votes) all of the current height votes.
    /// - drops votes from earlier heights.
    /// - retains future votes in the cache.
    fn get_current_height_votes(&mut self, height: BlockNumber) -> Vec<Vote> {
        loop {
            let Some(entry) = self.future_votes.first_entry() else {
                return Vec::new();
            };
            match entry.key().cmp(&height) {
                std::cmp::Ordering::Greater => return Vec::new(),
                std::cmp::Ordering::Equal => return entry.remove(),
                std::cmp::Ordering::Less => {
                    entry.remove();
                }
            }
        }
    }

    /// Checks if a cached proposal already exists (with correct height)
    /// - returns the proposals for the height if they exist and removes them from the cache.
    /// - cleans up any proposals from earlier heights.
    fn get_current_height_proposals(
        &mut self,
        height: BlockNumber,
    ) -> Vec<(ProposalInit, mpsc::Receiver<ContextT::ProposalPart>)> {
        loop {
            let Some(entry) = self.future_proposals_cache.first_entry() else {
                return Vec::new();
            };
            match entry.key().cmp(&height) {
                std::cmp::Ordering::Greater => return Vec::new(),
                std::cmp::Ordering::Equal => {
                    let round_to_proposals = entry.remove();
                    return round_to_proposals.into_values().collect();
                }
                std::cmp::Ordering::Less => {
                    entry.remove();
                }
            }
        }
    }

    /// Clears any cached messages for the given height or any lower height.
    fn clear_past_and_current_heights(&mut self, height: BlockNumber) {
        self.get_current_height_votes(height);
        self.get_current_height_proposals(height);
    }

    /// Caches a vote for a future height.
    fn cache_future_vote(&mut self, vote: Vote) -> Result<(), Box<EquivocationVoteReport>> {
        let votes = self.future_votes.entry(vote.height).or_default();
        // Find a vote in the list with the same type, round, and voter. If found, do not add it to
        // list.
        let existing_vote = votes.iter().find(|v| {
            v.vote_type == vote.vote_type && v.round == vote.round && v.voter == vote.voter
        });
        if let Some(existing_vote) = existing_vote {
            // If the two votes are identical, we just ignore this.
            if existing_vote == &vote {
                Ok(())
            } else {
                // Otherwise, we report equivocation.
                Err(Box::new(EquivocationVoteReport {
                    cached_vote: existing_vote.clone(),
                    new_vote: vote,
                }))
            }
        } else {
            // If no duplicate vote was found, we add the vote to the list.
            votes.push(vote);
            Ok(())
        }
    }

    /// Caches a proposal for a future height.
    fn cache_future_proposal(
        &mut self,
        init: ProposalInit,
        content_receiver: mpsc::Receiver<ContextT::ProposalPart>,
    ) {
        self.future_proposals_cache
            .entry(init.height)
            .or_default()
            .entry(init.round)
            .or_insert((init, content_receiver));
    }

    fn report_max_cached_block_number_metric(&self, height: BlockNumber) {
        // If nothing is cached use current height as "max".
        let max_cached_block_number = self.future_proposals_cache.keys().max().unwrap_or(&height);
        CONSENSUS_MAX_CACHED_BLOCK_NUMBER.set_lossy(max_cached_block_number.0);
    }

    fn report_cached_votes_metric(&self, height: BlockNumber) {
        let cached_votes_count =
            self.future_votes.get(&height).map(|votes| votes.len()).unwrap_or(0);
        CONSENSUS_CACHED_VOTES.set_lossy(cached_votes_count);
    }

    fn should_cache_msg(
        &self,
        current_height: &BlockNumber,
        current_round: Round,
        msg_height: BlockNumber,
        msg_round: Round,
        msg_description: &str,
    ) -> bool {
        let limits = &self.future_msg_limit;
        let height_diff = msg_height.saturating_sub(current_height.0);

        let should_cache = height_diff <= limits.future_height_limit.into()
            // For current height, check against current round + future_round_limit
            && (height_diff == 0 && msg_round <= current_round + limits.future_round_limit
                // For future heights, check absolute round limit
                || height_diff > 0 && msg_round <= limits.future_height_round_limit);

        if !should_cache {
            warn!(
                "Dropping {} for height={} round={} when current_height={} current_round={} - \
                 limits: future_height={}, future_height_round={}, future_round={}",
                msg_description,
                msg_height,
                msg_round,
                current_height,
                current_round,
                limits.future_height_limit,
                limits.future_height_round_limit,
                limits.future_round_limit
            );
        }

        should_cache
    }

    fn should_cache_proposal(
        &self,
        current_height: &BlockNumber,
        current_round: Round,
        proposal: &ProposalInit,
    ) -> bool {
        self.should_cache_msg(
            current_height,
            current_round,
            proposal.height,
            proposal.round,
            "proposal",
        )
    }

    fn should_cache_vote(
        &self,
        current_height: &BlockNumber,
        current_round: Round,
        vote: &Vote,
    ) -> bool {
        self.should_cache_msg(current_height, current_round, vote.height, vote.round, "vote")
    }
}

/// Runs Tendermint repeatedly across different heights. Handles issues which are not explicitly
/// part of the single height consensus algorithm (e.g. messages from future heights).
struct MultiHeightManager<ContextT: ConsensusContext> {
    consensus_config: ConsensusConfig,
    quorum_type: QuorumType,
    last_voted_height_at_initialization: Option<BlockNumber>,
    // The reason for this Arc<Mutex> we cannot share this instance mutably  with
    // SingleHeightConsensus despite them not ever using it at the same time in a simpler way, due
    // rust limitations.
    voted_height_storage: Arc<Mutex<dyn HeightVotedStorageTrait>>,
    #[allow(dead_code)]
    committee_provider: Arc<dyn CommitteeProvider>,
    // Proposal content streams keyed by (height, round)
    current_height_proposals_streams:
        BTreeMap<(BlockNumber, Round), mpsc::Receiver<ContextT::ProposalPart>>,
    cache: ConsensusCache<ContextT>,
}

impl<ContextT: ConsensusContext> MultiHeightManager<ContextT> {
    /// Create a new consensus manager.
    pub(crate) async fn new(
        consensus_config: ConsensusConfig,
        quorum_type: QuorumType,
        voted_height_storage: Arc<Mutex<dyn HeightVotedStorageTrait>>,
        committee_provider: Arc<dyn CommitteeProvider>,
    ) -> Self {
        let last_voted_height_at_initialization = voted_height_storage
            .lock()
            .await
            .get_prev_voted_height()
            .expect("Failed to get previous voted height from storage");
        let future_msg_limit = consensus_config.dynamic_config.future_msg_limit;
        Self {
            consensus_config,
            quorum_type,
            last_voted_height_at_initialization,
            voted_height_storage,
            committee_provider,
            current_height_proposals_streams: BTreeMap::new(),
            cache: ConsensusCache::new(future_msg_limit),
        }
    }

    /// Apply the full dynamic consensus configuration. Call only between heights.
    pub(crate) fn set_dynamic_config(&mut self, cfg: ConsensusDynamicConfig) {
        self.cache.set_future_msg_limit(cfg.future_msg_limit);
        self.consensus_config.dynamic_config = cfg;
    }

    /// Run the consensus algorithm for a single height.
    ///
    /// IMPORTANT: An error implies that consensus cannot continue, not just that the current height
    /// failed.
    ///
    /// A height of consensus ends either when the node learns of a decision, either by consensus
    /// directly or via the sync protocol.
    /// - In both cases, the height is committed to the context.
    ///
    /// This is the "top level" task of consensus, which is able to multiplex across activities:
    /// network messages and self generated events.
    ///
    /// Assumes that `height` is monotonically increasing across calls.
    ///
    /// Inputs - see [`run_consensus`].
    #[instrument(skip_all, fields(height=%height.0), level = "error")]
    pub(crate) async fn run_height(
        &mut self,
        context: &mut ContextT,
        height: BlockNumber,
        broadcast_channels: &mut BroadcastVoteChannel,
        proposals_receiver: &mut mpsc::Receiver<mpsc::Receiver<ContextT::ProposalPart>>,
    ) -> Result<RunHeightRes, ConsensusError> {
        info!("Running consensus for height {}.", height);

        let consensus_result =
            self.run_height_inner(context, height, broadcast_channels, proposals_receiver).await;

        let res = match consensus_result {
            Ok(ok) => match ok {
                RunHeightRes::Decision(decision) => {
                    // Commit decision to context.
                    context.decision_reached(height, decision.block).await?;
                    RunHeightRes::Decision(decision)
                }
                RunHeightRes::Sync => RunHeightRes::Sync,
            },

            Err(err) => match err {
                e @ ConsensusError::BatcherError(_) | e @ ConsensusError::CommitteeError(_) => {
                    error!(
                        "Error while running consensus for height {height}, fallback to sync: {e}"
                    );
                    self.wait_until_sync_reaches_height(height, context).await;
                    RunHeightRes::Sync
                }
                e @ ConsensusError::InternalNetworkError(_) => {
                    // The node is missing required components/data and cannot continue
                    // participating in the consensus. A fix and node restart are required.
                    return Err(e);
                }
            },
        };

        // Cleanup after height completion.
        self.cleanup_post_height(height, broadcast_channels, proposals_receiver).await?;

        Ok(res)
    }

    /// Continiously attempts to sync to the given height and waits until it succeeds.
    async fn wait_until_sync_reaches_height(
        &mut self,
        height: BlockNumber,
        context: &mut ContextT,
    ) {
        loop {
            if context.try_sync(height).await {
                debug!("Synced to {height}");
                break;
            }
            tokio::time::sleep(self.consensus_config.dynamic_config.sync_retry_interval).await;
            debug_every_n_ms!(1000, "Retrying sync to {height}");
            trace!("Retrying sync to {height}");
        }
    }

    async fn run_height_inner(
        &mut self,
        context: &mut ContextT,
        height: BlockNumber,
        broadcast_channels: &mut BroadcastVoteChannel,
        proposals_receiver: &mut mpsc::Receiver<mpsc::Receiver<ContextT::ProposalPart>>,
    ) -> Result<RunHeightRes, ConsensusError> {
        CONSENSUS_BLOCK_NUMBER.set_lossy(height.0);
        self.cache.report_max_cached_block_number_metric(height);

        if let Some(sync_result) = self.check_and_wait_for_sync(context, height).await {
            return Ok(sync_result);
        }

        let (mut shc, mut shc_events) = self.initialize_single_height_consensus(height).await?;

        if let Some(decision) = self
            .process_start_height(context, height, &mut shc, &mut shc_events, broadcast_channels)
            .await?
        {
            error!("Decision reached before executing requests. {:?}", decision);
            return Ok(RunHeightRes::Decision(decision));
        }

        self.process_consensus_events(
            context,
            height,
            &mut shc,
            &mut shc_events,
            broadcast_channels,
            proposals_receiver,
        )
        .await
    }

    /// Check if we need to sync and wait if necessary.
    /// Returns Some(RunHeightRes::Sync) if sync height was learned via sync, None otherwise.
    async fn check_and_wait_for_sync(
        &mut self,
        context: &mut ContextT,
        height: BlockNumber,
    ) -> Option<RunHeightRes> {
        // If we already voted for this height, do not proceed until we sync to this height.
        // Otherwise, just check if we can sync to this height, immediately. If not, proceed with
        // consensus.
        if self
            .last_voted_height_at_initialization
            .is_some_and(|last_voted_height| last_voted_height >= height)
        {
            // TODO(guy.f): Add this as a proposal failure with the reason in the prposal failure
            // metrics.
            info!(
                "Current height ({height}) is less than or equal to the last voted height at \
                 initialization ({}). Waiting for sync.",
                self.last_voted_height_at_initialization.unwrap().0
            );
            self.wait_until_sync_reaches_height(height, context).await;
            return Some(RunHeightRes::Sync);
        } else if context.try_sync(height).await {
            return Some(RunHeightRes::Sync);
        }
        None
    }

    /// Initialize consensus for a height: get validators, create SHC, and set up events.
    async fn initialize_single_height_consensus(
        &mut self,
        height: BlockNumber,
    ) -> Result<
        (SingleHeightConsensus, FuturesUnordered<BoxFuture<'static, StateMachineEvent>>),
        ConsensusError,
    > {
        let committee = self
            .committee_provider
            .get_committee(height)
            .await
            .map_err(|e| ConsensusError::CommitteeError(e.to_string()))?;

        let validators: Vec<_> = committee.members().iter().map(|s| s.address).collect();
        let is_observer = !validators.contains(&self.consensus_config.dynamic_config.validator_id);
        info!(
            "START_HEIGHT: running consensus for height {:?}. is_observer: {}, validators: {:?}",
            height, is_observer, validators,
        );

        let shc = SingleHeightConsensus::new(
            height,
            is_observer,
            self.consensus_config.dynamic_config.validator_id,
            validators,
            self.quorum_type,
            self.consensus_config.dynamic_config.timeouts.clone(),
            committee,
            self.consensus_config.dynamic_config.require_virtual_proposer_vote,
        );
        let shc_events = FuturesUnordered::new();

        Ok((shc, shc_events))
    }

    /// Process the start of a height: call shc.start, process cached proposals/votes, and execute
    /// initial requests. Returns Some(Decision) if a decision was reached (with error logging),
    /// None otherwise.
    async fn process_start_height(
        &mut self,
        context: &mut ContextT,
        height: BlockNumber,
        shc: &mut SingleHeightConsensus,
        shc_events: &mut FuturesUnordered<BoxFuture<'static, StateMachineEvent>>,
        broadcast_channels: &mut BroadcastVoteChannel,
    ) -> Result<Option<Decision>, ConsensusError> {
        self.cache.report_cached_votes_metric(height);
        let mut pending_requests = shc.start();

        let cached_proposals = self.cache.get_current_height_proposals(height);
        trace!("Cached proposals for height {}: {:?}", height, cached_proposals);
        for (init, content_receiver) in cached_proposals {
            let new_requests =
                self.handle_proposal_known_block_info(height, shc, init, content_receiver).await;
            pending_requests.extend(new_requests);
        }

        let cached_votes = self.cache.get_current_height_votes(height);
        trace!("Cached votes for height {}: {:?}", height, cached_votes);
        for msg in cached_votes {
            let new_requests = shc.handle_vote(msg);
            pending_requests.extend(new_requests);
        }

        // Reflect initial height/round to context before executing requests.
        context.set_height_and_round(height, shc.current_round()).await?;
        self.execute_requests(
            context,
            height,
            shc.committee(),
            pending_requests,
            shc_events,
            broadcast_channels,
        )
        .await
    }

    /// Main consensus loop: handles incoming proposals, votes, events, and sync checks.
    async fn process_consensus_events(
        &mut self,
        context: &mut ContextT,
        height: BlockNumber,
        shc: &mut SingleHeightConsensus,
        shc_events: &mut FuturesUnordered<BoxFuture<'static, StateMachineEvent>>,
        broadcast_channels: &mut BroadcastVoteChannel,
        proposals_receiver: &mut mpsc::Receiver<mpsc::Receiver<ContextT::ProposalPart>>,
    ) -> Result<RunHeightRes, ConsensusError> {
        let clock = DefaultClock;
        let sync_retry_interval = self.consensus_config.dynamic_config.sync_retry_interval;
        let mut sync_poll_deadline = clock.now() + sync_retry_interval;
        loop {
            self.cache.report_max_cached_block_number_metric(height);
            let requests = tokio::select! {
                message = broadcast_channels.broadcasted_messages_receiver.next() => {
                    let message = message.ok_or_else(|| ConsensusError::InternalNetworkError("Votes channel should never be closed".to_string()))?;
                    self.handle_vote(height, Some(shc), message, broadcast_channels).await?
                },
                content_receiver = proposals_receiver.next() => {
                    let content_receiver = content_receiver.ok_or_else(|| ConsensusError::InternalNetworkError("Proposals channel should never be closed".to_string()))?;
                    self.handle_proposal(
                        height,
                        Some(shc),
                        content_receiver
                    )
                    .await?
                },
                Some(shc_event) = shc_events.next() => {
                    shc.handle_event(shc_event)
                },
                // Using sleep_until to make sure that we won't restart the sleep due to other
                // events occuring.
                _ = clock.sleep_until(sync_poll_deadline) => {
                    sync_poll_deadline += sync_retry_interval;
                    if context.try_sync(height).await {
                        return Ok(RunHeightRes::Sync);
                    }
                    continue;
                }
            };
            // Reflect current height/round to context.
            context.set_height_and_round(height, shc.current_round()).await?;
            if let Some(decision) = self
                .execute_requests(
                    context,
                    height,
                    shc.committee(),
                    requests,
                    shc_events,
                    broadcast_channels,
                )
                .await?
            {
                return Ok(RunHeightRes::Decision(decision));
            }
        }
    }

    async fn cleanup_post_height(
        &mut self,
        height: BlockNumber,
        broadcast_channels: &mut BroadcastVoteChannel,
        proposals_receiver: &mut mpsc::Receiver<mpsc::Receiver<ContextT::ProposalPart>>,
    ) -> Result<(), ConsensusError> {
        // Clear any existing votes and proposals for previous heights as well as the current just
        // completed height using the dedicated cache manager.
        self.cache.clear_past_and_current_heights(height);

        // Clear any votes/proposals that might have arrived *during* the final await/cleanup,
        // which still belong to the completed height or lower.
        while let Some(Some(message)) =
            broadcast_channels.broadcasted_messages_receiver.next().now_or_never()
        {
            // Discard any votes for this height or lower by sending a None SHC.
            self.handle_vote(height, None, message, broadcast_channels).await?;
        }
        while let Ok(Some(content_receiver)) = proposals_receiver.try_next() {
            self.handle_proposal(height, None, content_receiver).await?;
        }

        // Height completed; clear any content streams associated with current and lower heights.
        self.current_height_proposals_streams.retain(|(h, _), _| *h > height);

        Ok(())
    }

    // Handle a new proposal receiver from the network.
    // shc - None if the height was just completed and we should drop the message.
    async fn handle_proposal(
        &mut self,
        height: BlockNumber,
        shc: Option<&mut SingleHeightConsensus>,
        mut content_receiver: mpsc::Receiver<ContextT::ProposalPart>,
    ) -> Result<Requests, ConsensusError> {
        CONSENSUS_PROPOSALS_RECEIVED.increment(1);
        // Get the first message to verify the init was sent.
        let Some(first_part) = content_receiver.try_next().ok().flatten() else {
            error!(
                "Couldn't get the first part of the proposal. Channel is unexpectedly empty. \
                 Dropping proposal."
            );
            return Ok(VecDeque::new());
        };

        let init: ProposalInit = match first_part.try_into() {
            Ok(init) => init,
            Err(e) => {
                warn!("Failed to parse incoming init. Dropping proposal: {e}");
                return Ok(VecDeque::new());
            }
        };

        let ord = init.height.cmp(&height);
        match ord {
            std::cmp::Ordering::Less => {
                trace!("Drop proposal from past height. {:?}", init);
                Ok(VecDeque::new())
            }
            std::cmp::Ordering::Greater | std::cmp::Ordering::Equal => {
                let Ok(proposer) =
                    get_proposer_for_height(&self.committee_provider, init.height, init.round)
                        .await
                else {
                    warn!(
                        "VIRTUAL_PROPOSER_LOOKUP_FAILED: Failed to determine virtual proposer for \
                         height {} round {}. Dropping proposal.",
                        init.height.0, init.round
                    );
                    return Ok(VecDeque::new());
                };
                if proposer != init.proposer {
                    warn!(
                        "Invalid proposer for height {} and round {}: expected {:?}, got {:?}",
                        init.height.0, init.round, proposer, init.proposer
                    );
                    return Ok(VecDeque::new());
                }
                if ord == std::cmp::Ordering::Greater {
                    if self.cache.should_cache_proposal(&height, 0, &init) {
                        debug!("Received a proposal for a future height. {:?}", init);
                        // Note: new proposals with the same height/round will be ignored.
                        //
                        // TODO(matan): This only work for trusted peers. In the case of
                        // possibly malicious peers this is a
                        // possible DoS attack (malicious
                        // users can insert invalid/bad/malicious proposals before
                        // "good" nodes can propose).
                        //
                        // When moving to version 1.0 make sure this is addressed.
                        self.cache.cache_future_proposal(init, content_receiver);
                    }
                    Ok(VecDeque::new())
                } else {
                    match shc {
                        Some(shc) => {
                            if self.cache.should_cache_proposal(&height, shc.current_round(), &init)
                            {
                                Ok(self
                                    .handle_proposal_known_block_info(
                                        height,
                                        shc,
                                        init,
                                        content_receiver,
                                    )
                                    .await)
                            } else {
                                Ok(VecDeque::new())
                            }
                        }
                        None => {
                            trace!("Drop proposal from just completed height. {:?}", init);
                            Ok(VecDeque::new())
                        }
                    }
                }
            }
        }
    }

    async fn handle_proposal_known_block_info(
        &mut self,
        height: BlockNumber,
        shc: &mut SingleHeightConsensus,
        init: ProposalInit,
        content_receiver: mpsc::Receiver<ContextT::ProposalPart>,
    ) -> Requests {
        // Store the stream; requests will reference it by (height, round)
        self.current_height_proposals_streams.insert((height, init.round), content_receiver);
        shc.handle_proposal(init)
    }

    // TODO(guyn): send back a report with the relevant data for why we report the peer.
    async fn report_peer(
        &self,
        broadcast_channels: &mut BroadcastVoteChannel,
        metadata: &BroadcastedMessageMetadata,
        reason: BadPeerReason,
        penalty_card: PenaltyCard,
    ) {
        let bad_peer_report =
            BadPeerReport { peer_id: metadata.originator_id.clone(), reason, penalty_card };
        if broadcast_channels
            .broadcast_topic_client
            .report_peer(metadata.clone(), bad_peer_report)
            .await
            .is_err()
        {
            error!("Unable to report peer. {:?}", metadata);
        }
    }

    // Handle a single consensus message.
    // shc - None if the height was just completed and we should drop the message.
    async fn handle_vote(
        &mut self,
        height: BlockNumber,
        shc: Option<&mut SingleHeightConsensus>,
        vote: (Result<Vote, ProtobufConversionError>, BroadcastedMessageMetadata),
        broadcast_channels: &mut BroadcastVoteChannel,
    ) -> Result<Requests, ConsensusError> {
        let (message, metadata) = match vote {
            (Ok(message), metadata) => {
                // TODO(matan): Hold onto report_sender for use in later errors by SHC.
                if broadcast_channels
                    .broadcast_topic_client
                    .continue_propagation(&metadata)
                    .now_or_never()
                    .is_none()
                {
                    error!("Unable to send continue_propagation. {:?}", metadata);
                }
                (message, metadata)
            }
            (Err(e), metadata) => {
                // Failed to parse consensus message. Report the peer and drop the vote.
                self.report_peer(
                    broadcast_channels,
                    &metadata,
                    BadPeerReason::ConversionError(e.to_string()),
                    PenaltyCard::Yellow,
                )
                .await;
                warn!(
                    "Failed to parse incoming consensus vote, dropping vote. Error: {e}. Vote \
                     metadata: {metadata:?}"
                );
                return Ok(VecDeque::new());
            }
        };

        // TODO(matan): We need to figure out an actual caching strategy under 2 constraints:
        // 1. Malicious - must be capped so a malicious peer can't DoS us.
        // 2. Parallel proposals - we may send/receive a proposal for (H+1, 0).
        match message.height.cmp(&height) {
            std::cmp::Ordering::Greater => {
                if self.cache.should_cache_vote(&height, 0, &message) {
                    trace!("Cache message for a future height. {:?}", message);
                    let duplicate_report = self.cache.cache_future_vote(message);
                    if let Err(duplicate_report) = duplicate_report {
                        warn!("Duplicate vote found: {:?}", duplicate_report);
                        self.report_peer(
                            broadcast_channels,
                            &metadata,
                            BadPeerReason::EquivocationVote(format!("{duplicate_report:?}")),
                            PenaltyCard::Red,
                        )
                        .await;
                    }
                }
                Ok(VecDeque::new())
            }
            std::cmp::Ordering::Less => {
                trace!("Drop message from past height. {:?}", message);
                Ok(VecDeque::new())
            }
            std::cmp::Ordering::Equal => match shc {
                Some(shc) => {
                    if self.cache.should_cache_vote(&height, shc.current_round(), &message) {
                        Ok(shc.handle_vote(message))
                    } else {
                        Ok(VecDeque::new())
                    }
                }
                None => {
                    trace!("Drop message from just completed height. {:?}", message);
                    Ok(VecDeque::new())
                }
            },
        }
    }

    async fn execute_requests(
        &mut self,
        context: &mut ContextT,
        height: BlockNumber,
        committee: Arc<dyn CommitteeTrait>,
        mut requests: VecDeque<SMRequest>,
        shc_events: &mut FuturesUnordered<BoxFuture<'static, StateMachineEvent>>,
        broadcast_channels: &mut BroadcastVoteChannel,
    ) -> Result<Option<Decision>, ConsensusError> {
        while let Some(request) = requests.pop_front() {
            match request {
                SMRequest::DecisionReached(decision) => {
                    return Ok(Some(decision));
                }
                _ => {
                    if let Some(fut) = self
                        .run_request(context, height, &committee, request, broadcast_channels)
                        .await?
                    {
                        shc_events.push(fut);
                    }
                }
            }
        }

        Ok(None)
    }

    async fn run_request(
        &mut self,
        context: &mut ContextT,
        height: BlockNumber,
        committee: &Arc<dyn CommitteeTrait>,
        request: SMRequest,
        _broadcast_channels: &mut BroadcastVoteChannel,
    ) -> Result<Option<BoxFuture<'static, StateMachineEvent>>, ConsensusError> {
        let timeouts = &self.consensus_config.dynamic_config.timeouts;
        match request {
            SMRequest::StartBuildProposal(round) => {
                let Ok(virtual_proposer) = committee.get_proposer(height, round) else {
                    warn!(
                        "VIRTUAL_PROPOSER_LOOKUP_FAILED: Failed to determine virtual proposer for \
                         height {height} round {round}. Proposal building will fail.",
                    );
                    let fut =
                        async move { StateMachineEvent::FinishedBuilding(None, round) }.boxed();
                    return Ok(Some(fut));
                };
                let build_param =
                    BuildParam { height, round, proposer: virtual_proposer, valid_round: None };
                // TODO(Asmaa): Reconsider: we should keep the builder's timeout bounded
                // independently of the consensus proposal timeout. We currently use the base
                // (round 0) proposal timeout for building to avoid giving the Batcher more time
                // when proposal time is extended for consensus.
                let timeout = timeouts.get_proposal_timeout(0);
                let receiver = context.build_proposal(build_param, timeout).await?;
                let fut = async move {
                    let proposal_id = receiver.await.ok();
                    StateMachineEvent::FinishedBuilding(proposal_id, round)
                }
                .boxed();
                Ok(Some(fut))
            }
            SMRequest::StartValidateProposal(init) => {
                // Look up the stored stream.
                let round = init.round;
                let valid_round = init.valid_round;
                let key = (height, round);
                if let Some(stream) = self.current_height_proposals_streams.remove(&key) {
                    let timeout = timeouts.get_proposal_timeout(round);
                    let receiver = context.validate_proposal(init, timeout, stream).await;
                    let fut = async move {
                        let proposal_id = receiver.await.ok();
                        StateMachineEvent::FinishedValidation(proposal_id, round, valid_round)
                    }
                    .boxed();
                    Ok(Some(fut))
                } else {
                    // No stream available; ignore.
                    Ok(None)
                }
            }
            SMRequest::BroadcastVote(vote) => {
                trace!("Writing voted height {} to storage", height);
                self.voted_height_storage
                    .lock()
                    .await
                    .set_prev_voted_height(height)
                    .expect("Failed to write voted height {self.height} to storage");
                info!("Broadcasting {vote:?}");
                context.broadcast(vote.clone()).await?;
                // Schedule a rebroadcast after the appropriate timeout.
                let duration = match vote.vote_type {
                    VoteType::Prevote => timeouts.get_prevote_timeout(0),
                    VoteType::Precommit => timeouts.get_precommit_timeout(0),
                };
                let fut = async move {
                    tokio::time::sleep(duration).await;
                    StateMachineEvent::VoteBroadcasted(vote)
                }
                .boxed();
                Ok(Some(fut))
            }
            SMRequest::ScheduleTimeout(step, round) => {
                let (duration, event) = match step {
                    Step::Propose => (
                        timeouts.get_proposal_timeout(round),
                        StateMachineEvent::TimeoutPropose(round),
                    ),
                    Step::Prevote => (
                        timeouts.get_prevote_timeout(round),
                        StateMachineEvent::TimeoutPrevote(round),
                    ),
                    Step::Precommit => (
                        timeouts.get_precommit_timeout(round),
                        StateMachineEvent::TimeoutPrecommit(round),
                    ),
                };
                let fut = async move {
                    tokio::time::sleep(duration).await;
                    event
                }
                .boxed();
                Ok(Some(fut))
            }
            SMRequest::Repropose(proposal_id, build_param) => {
                context.repropose(proposal_id, build_param).await;
                CONSENSUS_REPROPOSALS.increment(1);
                Ok(None)
            }
            SMRequest::DecisionReached(_) => {
                unreachable!("DecisionReached request should be handled in execute_requests");
            }
        }
    }
}

/// Fetches the committee for the given height and returns the proposer for that height and round.
async fn get_proposer_for_height(
    committee_provider: &Arc<dyn CommitteeProvider>,
    height: BlockNumber,
    round: Round,
) -> Result<ValidatorId, ConsensusError> {
    let committee = committee_provider
        .get_committee(height)
        .await
        .map_err(|e| ConsensusError::CommitteeError(e.to_string()))?;
    committee.get_proposer(height, round).map_err(|e| ConsensusError::CommitteeError(e.to_string()))
}
