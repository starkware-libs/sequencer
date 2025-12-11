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
use std::sync::{Arc, Mutex};

use apollo_config_manager_types::communication::SharedConfigManagerClient;
use apollo_consensus_config::config::{
    ConsensusConfig,
    ConsensusDynamicConfig,
    FutureMsgLimitsConfig,
};
use apollo_infra_utils::debug_every_n_sec;
use apollo_network::network_manager::BroadcastTopicClientTrait;
use apollo_network_types::network_types::BroadcastedMessageMetadata;
use apollo_protobuf::consensus::{ProposalInit, Vote, VoteType};
use apollo_protobuf::converters::ProtobufConversionError;
use apollo_time::time::{Clock, ClockExt, DefaultClock};
use futures::channel::mpsc;
use futures::future::BoxFuture;
use futures::stream::FuturesUnordered;
use futures::{FutureExt, StreamExt};
use starknet_api::block::BlockNumber;
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
use crate::single_height_consensus::{ShcReturn, SingleHeightConsensus};
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
    );
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
                let proposer = context.proposer(current_height, round);

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

/// Manages votes and proposals for future heights.
#[derive(Debug)]
struct ConsensusCache<ContextT: ConsensusContext> {
    // Mapping: { Height : Vec<Vote> }
    future_votes: BTreeMap<BlockNumber, Vec<Vote>>,
    // Mapping: { Height : { Round : (Init, Receiver)}}
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
    fn cache_future_vote(&mut self, vote: Vote) {
        self.future_votes.entry(vote.height).or_default().push(vote);
    }

    /// Caches a proposal for a future height.
    fn cache_future_proposal(
        &mut self,
        proposal_init: ProposalInit,
        content_receiver: mpsc::Receiver<ContextT::ProposalPart>,
    ) {
        self.future_proposals_cache
            .entry(proposal_init.height)
            .or_default()
            .entry(proposal_init.round)
            .or_insert((proposal_init, content_receiver));
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
#[derive(Debug)]
struct MultiHeightManager<ContextT: ConsensusContext> {
    consensus_config: ConsensusConfig,
    quorum_type: QuorumType,
    last_voted_height_at_initialization: Option<BlockNumber>,
    // The reason for this Arc<Mutex> we cannot share this instance mutably  with
    // SingleHeightConsensus despite them not ever using it at the same time in a simpler way, due
    // rust limitations.
    voted_height_storage: Arc<Mutex<dyn HeightVotedStorageTrait>>,
    // Proposal content streams keyed by (height, round)
    current_height_proposals_streams:
        BTreeMap<(BlockNumber, Round), mpsc::Receiver<ContextT::ProposalPart>>,
    cache: ConsensusCache<ContextT>,
}

impl<ContextT: ConsensusContext> MultiHeightManager<ContextT> {
    /// Create a new consensus manager.
    pub(crate) fn new(
        consensus_config: ConsensusConfig,
        quorum_type: QuorumType,
        voted_height_storage: Arc<Mutex<dyn HeightVotedStorageTrait>>,
    ) -> Self {
        let last_voted_height_at_initialization = voted_height_storage
            .lock()
            .expect("Lock should never be poisoned")
            .get_prev_voted_height()
            .expect("Failed to get previous voted height from storage");
        let future_msg_limit = consensus_config.dynamic_config.future_msg_limit;
        Self {
            consensus_config,
            quorum_type,
            last_voted_height_at_initialization,
            voted_height_storage,
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
        let res =
            self.run_height_inner(context, height, broadcast_channels, proposals_receiver).await?;

        // Commit in case of decision.
        if let RunHeightRes::Decision(decision) = &res {
            context.decision_reached(height, decision.block).await?;
        }

        // Cleanup after height completion.
        self.cleanup_post_height(context, height, broadcast_channels, proposals_receiver).await?;

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
            debug_every_n_sec!(1, "Retrying sync to {height}");
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

        if let Some(sync_result) = self.check_and_wait_for_sync(context, height).await? {
            return Ok(sync_result);
        }

        let (mut shc, mut shc_events) =
            self.initialize_single_height_consensus(context, height).await;

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
    /// Returns Some(RunHeightRes::Sync) if sync is needed, None otherwise.
    async fn check_and_wait_for_sync(
        &mut self,
        context: &mut ContextT,
        height: BlockNumber,
    ) -> Result<Option<RunHeightRes>, ConsensusError> {
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
            return Ok(Some(RunHeightRes::Sync));
        } else if context.try_sync(height).await {
            return Ok(Some(RunHeightRes::Sync));
        }
        Ok(None)
    }

    /// Initialize consensus for a height: get validators, create SHC, and set up events.
    async fn initialize_single_height_consensus(
        &mut self,
        context: &mut ContextT,
        height: BlockNumber,
    ) -> (SingleHeightConsensus, FuturesUnordered<BoxFuture<'static, StateMachineEvent>>) {
        let validators = context.validators(height).await;
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
        );
        let shc_events = FuturesUnordered::new();

        (shc, shc_events)
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
        let mut pending_requests = {
            let leader_fn = make_leader_fn(context, height);
            match shc.start(&leader_fn) {
                ShcReturn::Decision(decision) => {
                    // Start should generate either StartValidateProposal (validator) or
                    // StartBuildProposal (proposer). We do not enforce this
                    // since the Manager is intentionally not meant to
                    // understand consensus in detail.
                    return Ok(Some(decision));
                }
                ShcReturn::Requests(requests) => requests,
            }
        };

        let cached_proposals = self.cache.get_current_height_proposals(height);
        trace!("Cached proposals for height {}: {:?}", height, cached_proposals);
        for (init, content_receiver) in cached_proposals {
            match self
                .handle_proposal_known_init(context, height, shc, init, content_receiver)
                .await
            {
                ShcReturn::Decision(decision) => {
                    return Ok(Some(decision));
                }
                ShcReturn::Requests(new_requests) => pending_requests.extend(new_requests),
            }
        }

        let cached_votes = self.cache.get_current_height_votes(height);
        trace!("Cached votes for height {}: {:?}", height, cached_votes);
        for msg in cached_votes {
            let leader_fn = make_leader_fn(context, height);
            match shc.handle_vote(&leader_fn, msg) {
                ShcReturn::Decision(decision) => {
                    return Ok(Some(decision));
                }
                ShcReturn::Requests(new_requests) => pending_requests.extend(new_requests),
            }
        }

        // Reflect initial height/round to context before executing requests.
        context.set_height_and_round(height, shc.current_round()).await;
        self.execute_requests(context, height, pending_requests, shc_events, broadcast_channels)
            .await?;
        Ok(None)
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
            let shc_return = tokio::select! {
                message = broadcast_channels.broadcasted_messages_receiver.next() => {
                    self.handle_vote(context, height, Some(shc), message, broadcast_channels).await?
                },
                content_receiver = proposals_receiver.next() => {
                    self.handle_proposal(
                        context,
                        height,
                        Some(shc),
                        content_receiver
                    )
                    .await?
                },
                Some(shc_event) = shc_events.next() => {
                    let leader_fn = make_leader_fn(context, height);
                    shc.handle_event(&leader_fn, shc_event)
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
            context.set_height_and_round(height, shc.current_round()).await;
            match shc_return {
                ShcReturn::Decision(decision) => return Ok(RunHeightRes::Decision(decision)),
                ShcReturn::Requests(requests) => {
                    self.execute_requests(
                        context,
                        height,
                        requests,
                        shc_events,
                        broadcast_channels,
                    )
                    .await?;
                }
            }
        }
    }

    async fn cleanup_post_height(
        &mut self,
        context: &mut ContextT,
        height: BlockNumber,
        broadcast_channels: &mut BroadcastVoteChannel,
        proposals_receiver: &mut mpsc::Receiver<mpsc::Receiver<ContextT::ProposalPart>>,
    ) -> Result<(), ConsensusError> {
        // Clear any existing votes and proposals for previous heights as well as the current just
        // completed height using the dedicated cache manager.
        self.cache.clear_past_and_current_heights(height);

        // Clear any votes/proposals that might have arrived *during* the final await/cleanup,
        // which still belong to the completed height or lower.
        while let Some(message) =
            broadcast_channels.broadcasted_messages_receiver.next().now_or_never()
        {
            // Discard any votes for this height or lower by sending a None SHC.
            self.handle_vote(context, height, None, message, broadcast_channels).await?;
        }
        while let Ok(content_receiver) = proposals_receiver.try_next() {
            self.handle_proposal(context, height, None, content_receiver).await?;
        }

        // Height completed; clear any content streams associated with current and lower heights.
        self.current_height_proposals_streams.retain(|(h, _), _| *h > height);

        Ok(())
    }

    // Handle a new proposal receiver from the network.
    // shc - None if the height was just completed and we should drop the message.
    async fn handle_proposal(
        &mut self,
        context: &mut ContextT,
        height: BlockNumber,
        shc: Option<&mut SingleHeightConsensus>,
        content_receiver: Option<mpsc::Receiver<ContextT::ProposalPart>>,
    ) -> Result<ShcReturn, ConsensusError> {
        CONSENSUS_PROPOSALS_RECEIVED.increment(1);
        // Get the first message to verify the init was sent.
        let Some(mut content_receiver) = content_receiver else {
            return Err(ConsensusError::InternalNetworkError(
                "proposal receiver should never be closed".to_string(),
            ));
        };
        let Some(first_part) = content_receiver.try_next().map_err(|_| {
            ConsensusError::InternalNetworkError(
                "Stream handler must fill the first message before sending the stream".to_string(),
            )
        })?
        else {
            return Err(ConsensusError::InternalNetworkError(
                "Content receiver closed".to_string(),
            ));
        };
        let proposal_init: ProposalInit = first_part.try_into()?;

        match proposal_init.height.cmp(&height) {
            std::cmp::Ordering::Greater => {
                if self.cache.should_cache_proposal(&height, 0, &proposal_init) {
                    debug!("Received a proposal for a future height. {:?}", proposal_init);
                    // Note: new proposals with the same height/round will be ignored.
                    //
                    // TODO(matan): This only work for trusted peers. In the case of possibly
                    // malicious peers this is a possible DoS attack (malicious
                    // users can insert invalid/bad/malicious proposals before
                    // "good" nodes can propose).
                    //
                    // When moving to version 1.0 make sure this is addressed.
                    self.cache.cache_future_proposal(proposal_init, content_receiver);
                }
                Ok(ShcReturn::Requests(VecDeque::new()))
            }
            std::cmp::Ordering::Less => {
                trace!("Drop proposal from past height. {:?}", proposal_init);
                Ok(ShcReturn::Requests(VecDeque::new()))
            }
            std::cmp::Ordering::Equal => match shc {
                Some(shc) => {
                    if self.cache.should_cache_proposal(
                        &height,
                        shc.current_round(),
                        &proposal_init,
                    ) {
                        Ok(self
                            .handle_proposal_known_init(
                                context,
                                height,
                                shc,
                                proposal_init,
                                content_receiver,
                            )
                            .await)
                    } else {
                        Ok(ShcReturn::Requests(VecDeque::new()))
                    }
                }
                None => {
                    trace!("Drop proposal from just completed height. {:?}", proposal_init);
                    Ok(ShcReturn::Requests(VecDeque::new()))
                }
            },
        }
    }

    async fn handle_proposal_known_init(
        &mut self,
        context: &mut ContextT,
        height: BlockNumber,
        shc: &mut SingleHeightConsensus,
        proposal_init: ProposalInit,
        content_receiver: mpsc::Receiver<ContextT::ProposalPart>,
    ) -> ShcReturn {
        // Store the stream; requests will reference it by (height, round)
        self.current_height_proposals_streams
            .insert((height, proposal_init.round), content_receiver);
        let leader_fn = make_leader_fn(context, height);
        shc.handle_proposal(&leader_fn, proposal_init)
    }

    // Handle a single consensus message.
    // shc - None if the height was just completed and we should drop the message.
    async fn handle_vote(
        &mut self,
        context: &mut ContextT,
        height: BlockNumber,
        shc: Option<&mut SingleHeightConsensus>,
        vote: Option<(Result<Vote, ProtobufConversionError>, BroadcastedMessageMetadata)>,
        broadcast_channels: &mut BroadcastVoteChannel,
    ) -> Result<ShcReturn, ConsensusError> {
        let message = match vote {
            None => Err(ConsensusError::InternalNetworkError(
                "NetworkReceiver should never be closed".to_string(),
            )),
            Some((Ok(msg), metadata)) => {
                // TODO(matan): Hold onto report_sender for use in later errors by SHC.
                if broadcast_channels
                    .broadcast_topic_client
                    .continue_propagation(&metadata)
                    .now_or_never()
                    .is_none()
                {
                    error!("Unable to send continue_propagation. {:?}", metadata);
                }
                Ok(msg)
            }
            Some((Err(e), metadata)) => {
                // Failed to parse consensus message
                if broadcast_channels
                    .broadcast_topic_client
                    .report_peer(metadata.clone())
                    .now_or_never()
                    .is_none()
                {
                    error!("Unable to send report_peer. {:?}", metadata)
                }
                Err(e.into())
            }
        }?;

        // TODO(matan): We need to figure out an actual caching strategy under 2 constraints:
        // 1. Malicious - must be capped so a malicious peer can't DoS us.
        // 2. Parallel proposals - we may send/receive a proposal for (H+1, 0).
        match message.height.cmp(&height) {
            std::cmp::Ordering::Greater => {
                if self.cache.should_cache_vote(&height, 0, &message) {
                    trace!("Cache message for a future height. {:?}", message);
                    self.cache.cache_future_vote(message);
                }
                Ok(ShcReturn::Requests(VecDeque::new()))
            }
            std::cmp::Ordering::Less => {
                trace!("Drop message from past height. {:?}", message);
                Ok(ShcReturn::Requests(VecDeque::new()))
            }
            std::cmp::Ordering::Equal => match shc {
                Some(shc) => {
                    if self.cache.should_cache_vote(&height, shc.current_round(), &message) {
                        let leader_fn = make_leader_fn(context, height);
                        Ok(shc.handle_vote(&leader_fn, message))
                    } else {
                        Ok(ShcReturn::Requests(VecDeque::new()))
                    }
                }
                None => {
                    trace!("Drop message from just completed height. {:?}", message);
                    Ok(ShcReturn::Requests(VecDeque::new()))
                }
            },
        }
    }

    async fn execute_requests(
        &mut self,
        context: &mut ContextT,
        height: BlockNumber,
        mut requests: VecDeque<SMRequest>,
        shc_events: &mut FuturesUnordered<BoxFuture<'static, StateMachineEvent>>,
        broadcast_channels: &mut BroadcastVoteChannel,
    ) -> Result<(), ConsensusError> {
        while let Some(request) = requests.pop_front() {
            if let Some(fut) =
                self.run_request(context, height, request, broadcast_channels).await?
            {
                shc_events.push(fut);
            }
        }
        Ok(())
    }

    async fn run_request(
        &mut self,
        context: &mut ContextT,
        height: BlockNumber,
        request: SMRequest,
        _broadcast_channels: &mut BroadcastVoteChannel,
    ) -> Result<Option<BoxFuture<'static, StateMachineEvent>>, ConsensusError> {
        let timeouts = &self.consensus_config.dynamic_config.timeouts;
        match request {
            SMRequest::StartBuildProposal(round) => {
                let init = ProposalInit {
                    height,
                    round,
                    proposer: self.consensus_config.dynamic_config.validator_id,
                    valid_round: None,
                };
                // TODO(Asmaa): Reconsider: we should keep the builder's timeout bounded
                // independently of the consensus proposal timeout. We currently use the base
                // (round 0) proposal timeout for building to avoid giving the Batcher more time
                // when proposal time is extended for consensus.
                let timeout = timeouts.get_proposal_timeout(0);
                let receiver = context.build_proposal(init, timeout).await;
                let fut = async move {
                    let proposal_id = receiver.await.ok();
                    StateMachineEvent::FinishedBuilding(proposal_id, round)
                }
                .boxed();
                Ok(Some(fut))
            }
            SMRequest::StartValidateProposal(init) => {
                // Look up the stored stream.
                let key = (height, init.round);
                if let Some(stream) = self.current_height_proposals_streams.remove(&key) {
                    let timeout = timeouts.get_proposal_timeout(init.round);
                    let receiver = context.validate_proposal(init, timeout, stream).await;
                    let round = init.round;
                    let valid_round = init.valid_round;
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
                    .expect(
                        "Lock should never be poisoned because there should never be concurrent \
                         access.",
                    )
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
            SMRequest::Repropose(proposal_id, init) => {
                context.repropose(proposal_id, init).await;
                CONSENSUS_REPROPOSALS.increment(1);
                Ok(None)
            }
            SMRequest::DecisionReached(_, _) => {
                // Should be handled by SHC, not manager.
                Err(ConsensusError::InternalInconsistency(
                    "Manager received DecisionReached request".to_string(),
                ))
            }
        }
    }
}

/// Creates a closure that returns the proposer for a given round at the specified height.
fn make_leader_fn<ContextT: ConsensusContext>(
    context: &ContextT,
    height: BlockNumber,
) -> impl Fn(Round) -> ValidatorId + '_ {
    move |round| context.proposer(height, round)
}
