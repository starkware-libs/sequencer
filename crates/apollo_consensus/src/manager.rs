//! Top level of consensus, used to run multiple heights of consensus.
//!
//! [`run_consensus`] - This is the primary entrypoint for running the consensus component.
//!
//! [`MultiHeightManager`] - Runs consensus repeatedly across different heights using
//! [`run_height`](MultiHeightManager::run_height).

#[cfg(test)]
#[path = "manager_test.rs"]
mod manager_test;

use std::collections::BTreeMap;
use std::time::Duration;

use apollo_network::network_manager::BroadcastTopicClientTrait;
use apollo_network_types::network_types::BroadcastedMessageMetadata;
use apollo_protobuf::consensus::{ProposalInit, Vote};
use apollo_protobuf::converters::ProtobufConversionError;
use apollo_time::time::{sleep_until, Clock, DefaultClock};
use futures::channel::mpsc;
use futures::stream::FuturesUnordered;
use futures::{FutureExt, StreamExt};
use starknet_api::block::BlockNumber;
use tracing::{debug, error, info, instrument, trace, warn};

use crate::config::{FutureMsgLimitsConfig, TimeoutsConfig};
use crate::metrics::{
    register_metrics,
    CONSENSUS_BLOCK_NUMBER,
    CONSENSUS_CACHED_VOTES,
    CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS,
    CONSENSUS_DECISIONS_REACHED_BY_SYNC,
    CONSENSUS_MAX_CACHED_BLOCK_NUMBER,
    CONSENSUS_PROPOSALS_RECEIVED,
};
use crate::single_height_consensus::{ShcReturn, SingleHeightConsensus};
use crate::types::{BroadcastVoteChannel, ConsensusContext, ConsensusError, Decision, ValidatorId};
use crate::votes_threshold::QuorumType;

/// Arguments for running consensus.
#[derive(Clone, Debug)]
pub struct RunConsensusArguments {
    /// The height at which the node may participate in consensus (if it is a validator).
    pub start_active_height: BlockNumber,
    /// The height at which the node begins to run consensus.
    pub start_observe_height: BlockNumber,
    /// The ID of this node.
    pub validator_id: ValidatorId,
    /// Delay before starting consensus; allowing the network to connect to peers.
    pub consensus_delay: Duration,
    /// The timeouts for the consensus algorithm.
    pub timeouts: TimeoutsConfig,
    /// The interval to wait between sync retries.
    pub sync_retry_interval: Duration,
    /// Set to Byzantine by default. Using Honest means we trust all validators. Use with caution!
    pub quorum_type: QuorumType,
    /// Future message limits configuration.
    pub future_msg_limit: FutureMsgLimitsConfig,
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
#[instrument(skip_all, fields(validator_id=%run_consensus_args.validator_id), level = "error")]
pub async fn run_consensus<ContextT>(
    run_consensus_args: RunConsensusArguments,
    mut context: ContextT,
    mut vote_receiver: BroadcastVoteChannel,
    mut proposals_receiver: mpsc::Receiver<mpsc::Receiver<ContextT::ProposalPart>>,
) -> Result<(), ConsensusError>
where
    ContextT: ConsensusContext,
{
    info!("Running consensus, args: {:?}", run_consensus_args.clone());
    register_metrics();
    // Add a short delay to allow peers to connect and avoid "InsufficientPeers" error
    tokio::time::sleep(run_consensus_args.consensus_delay).await;
    assert!(run_consensus_args.start_observe_height <= run_consensus_args.start_active_height);
    let mut current_height = run_consensus_args.start_observe_height;
    let mut manager = MultiHeightManager::new(
        run_consensus_args.validator_id,
        run_consensus_args.sync_retry_interval,
        run_consensus_args.quorum_type,
        run_consensus_args.timeouts,
        run_consensus_args.future_msg_limit,
    );
    loop {
        let must_observer = current_height < run_consensus_args.start_active_height;
        match manager
            .run_height(
                &mut context,
                current_height,
                must_observer,
                &mut vote_receiver,
                &mut proposals_receiver,
            )
            .await?
        {
            RunHeightRes::Decision(decision) => {
                // We expect there to be under 100 validators, so this is a reasonable number of
                // precommits to print.
                let round = decision.precommits[0].round;
                let proposer = context.proposer(current_height, round);
                info!(
                    "DECISION_REACHED: Decision reached for round {} with proposer {}. {:?}",
                    round, proposer, decision
                );
                CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS.increment(1);
                context.decision_reached(decision.block, decision.precommits).await?;
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

/// Runs Tendermint repeatedly across different heights. Handles issues which are not explicitly
/// part of the single height consensus algorithm (e.g. messages from future heights).
#[derive(Debug)]
struct MultiHeightManager<ContextT: ConsensusContext> {
    validator_id: ValidatorId,
    future_votes: BTreeMap<u64, Vec<Vote>>,
    sync_retry_interval: Duration,
    quorum_type: QuorumType,
    // Mapping: { Height : { Round : (Init, Receiver)}}
    cached_proposals: BTreeMap<u64, BTreeMap<u32, ProposalReceiverTuple<ContextT::ProposalPart>>>,
    timeouts: TimeoutsConfig,
    future_msg_limit: FutureMsgLimitsConfig,
}

impl<ContextT: ConsensusContext> MultiHeightManager<ContextT> {
    /// Create a new consensus manager.
    pub(crate) fn new(
        validator_id: ValidatorId,
        sync_retry_interval: Duration,
        quorum_type: QuorumType,
        timeouts: TimeoutsConfig,
        future_msg_limit: FutureMsgLimitsConfig,
    ) -> Self {
        Self {
            validator_id,
            sync_retry_interval,
            quorum_type,
            future_votes: BTreeMap::new(),
            cached_proposals: BTreeMap::new(),
            timeouts,
            future_msg_limit,
        }
    }

    /// Run the consensus algorithm for a single height.
    ///
    /// A height of consensus ends either when the node learns of a decision, either by consensus
    /// directly or via the sync protocol.
    /// - An error implies that consensus cannot continue, not just that the current height failed.
    ///
    /// This is the "top level" task of consensus, which is able to multiplex across activities:
    /// network messages and self generated events.
    ///
    /// Assumes that `height` is monotonically increasing across calls.
    ///
    /// Inputs - see [`run_consensus`].
    /// - `must_observer`: Whether the node must observe or if it is allowed to be active (assuming
    ///   it is in the validator set).
    #[instrument(skip_all, fields(height=%height.0), level = "error")]
    pub(crate) async fn run_height(
        &mut self,
        context: &mut ContextT,
        height: BlockNumber,
        must_observer: bool,
        broadcast_channels: &mut BroadcastVoteChannel,
        proposals_receiver: &mut mpsc::Receiver<mpsc::Receiver<ContextT::ProposalPart>>,
    ) -> Result<RunHeightRes, ConsensusError> {
        let res = self
            .run_height_inner(
                context,
                height,
                must_observer,
                broadcast_channels,
                proposals_receiver,
            )
            .await?;

        // Clear any existing votes and proposals for previous heights as well as the current just
        // completed height.
        //
        // Networking layer assumes messages are handled in a timely fashion, otherwise we may build
        // up a backlog of useless messages. Similarly we don't want to waste space on old messages.
        // This is particularly important when there is a significant lag and we continually finish
        // heights immediately due to sync.

        // We use get_current_height_votes for its side effect of removing votes for lower
        // heights (we don't care about the actual votes).
        self.get_current_height_votes(height);
        while let Some(message) =
            broadcast_channels.broadcasted_messages_receiver.next().now_or_never()
        {
            // Discard any votes for this heigh or lower by sending a None SHC.
            self.handle_vote(context, height, None, message, broadcast_channels).await?;
        }
        // We call this method to filter out any proposals for previous/current heights (we don't
        // care about the returned proposals).
        self.get_current_height_proposals(height);
        while let Ok(content_receiver) = proposals_receiver.try_next() {
            self.handle_proposal(context, height, None, content_receiver).await?;
        }

        Ok(res)
    }

    async fn run_height_inner(
        &mut self,
        context: &mut ContextT,
        height: BlockNumber,
        must_observer: bool,
        broadcast_channels: &mut BroadcastVoteChannel,
        proposals_receiver: &mut mpsc::Receiver<mpsc::Receiver<ContextT::ProposalPart>>,
    ) -> Result<RunHeightRes, ConsensusError> {
        self.report_max_cached_block_number_metric(height);
        if context.try_sync(height).await {
            return Ok(RunHeightRes::Sync);
        }

        let validators = context.validators(height).await;
        let is_observer = must_observer || !validators.contains(&self.validator_id);
        info!(
            "START_HEIGHT: running consensus for height {:?}. is_observer: {}, validators: {:?}",
            height, is_observer, validators,
        );
        CONSENSUS_BLOCK_NUMBER.set_lossy(height.0);

        let mut shc = SingleHeightConsensus::new(
            height,
            is_observer,
            self.validator_id,
            validators,
            self.quorum_type,
            self.timeouts.clone(),
        );
        let mut shc_events = FuturesUnordered::new();

        match self.start_height(context, height, &mut shc).await? {
            ShcReturn::Decision(decision) => {
                return Ok(RunHeightRes::Decision(decision));
            }
            ShcReturn::Tasks(tasks) => {
                for task in tasks {
                    shc_events.push(task.run());
                }
            }
        }

        // Loop over incoming proposals, messages, and self generated events.
        let clock = DefaultClock;
        let mut sync_poll_deadline = clock.now() + self.sync_retry_interval;
        loop {
            self.report_max_cached_block_number_metric(height);
            let shc_return = tokio::select! {
                message = broadcast_channels.broadcasted_messages_receiver.next() => {
                    self.handle_vote(
                        context, height, Some(&mut shc), message, broadcast_channels).await?
                },
                content_receiver = proposals_receiver.next() => {
                    self.handle_proposal(context, height, Some(&mut shc), content_receiver).await?
                },
                Some(shc_event) = shc_events.next() => {
                    shc.handle_event(context, shc_event).await?
                },
                // Using sleep_until to make sure that we won't restart the sleep due to other
                // events occuring.
                _ = sleep_until(sync_poll_deadline, &clock) => {
                    sync_poll_deadline += self.sync_retry_interval;
                    if context.try_sync(height).await {
                        return Ok(RunHeightRes::Sync);
                    }
                    continue;
                }
            };

            match shc_return {
                ShcReturn::Decision(decision) => return Ok(RunHeightRes::Decision(decision)),
                ShcReturn::Tasks(tasks) => {
                    for task in tasks {
                        shc_events.push(task.run());
                    }
                }
            }
        }
    }

    async fn start_height(
        &mut self,
        context: &mut ContextT,
        height: BlockNumber,
        shc: &mut SingleHeightConsensus,
    ) -> Result<ShcReturn, ConsensusError> {
        CONSENSUS_CACHED_VOTES.set_lossy(self.future_votes.entry(height.0).or_default().len());
        let mut tasks = match shc.start(context).await? {
            decision @ ShcReturn::Decision(_) => {
                // Start should generate either TimeoutProposal (validator) or GetProposal
                // (proposer). We do not enforce this since the Manager is
                // intentionally not meant to understand consensus in detail.
                error!("Decision reached at start of height. {:?}", decision);
                return Ok(decision);
            }
            ShcReturn::Tasks(tasks) => tasks,
        };

        let cached_proposals = self.get_current_height_proposals(height);
        trace!("Cached proposals for height {}: {:?}", height, cached_proposals);
        for (init, content_receiver) in cached_proposals {
            match shc.handle_proposal(context, init, content_receiver).await? {
                decision @ ShcReturn::Decision(_) => return Ok(decision),
                ShcReturn::Tasks(new_tasks) => tasks.extend(new_tasks),
            }
        }

        let cached_votes = self.get_current_height_votes(height);
        trace!("Cached votes for height {}: {:?}", height, cached_votes);
        for msg in cached_votes {
            match shc.handle_vote(context, msg).await? {
                decision @ ShcReturn::Decision(_) => return Ok(decision),
                ShcReturn::Tasks(new_tasks) => tasks.extend(new_tasks),
            }
        }

        Ok(ShcReturn::Tasks(tasks))
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
                if self.should_cache_proposal(&height, 0, &proposal_init) {
                    debug!("Received a proposal for a future height. {:?}", proposal_init);
                    // Note: new proposals with the same height/round will be ignored.
                    //
                    // TODO(matan): This only work for trusted peers. In the case of possibly
                    // malicious peers this is a possible DoS attack (malicious
                    // users can insert invalid/bad/malicious proposals before
                    // "good" nodes can propose).
                    //
                    // When moving to version 1.0 make sure this is addressed.
                    self.cached_proposals
                        .entry(proposal_init.height.0)
                        .or_default()
                        .entry(proposal_init.round)
                        .or_insert((proposal_init, content_receiver));
                }
                Ok(ShcReturn::Tasks(Vec::new()))
            }
            std::cmp::Ordering::Less => {
                trace!("Drop proposal from past height. {:?}", proposal_init);
                Ok(ShcReturn::Tasks(Vec::new()))
            }
            std::cmp::Ordering::Equal => match shc {
                Some(shc) => {
                    if self.should_cache_proposal(&height, shc.current_round(), &proposal_init) {
                        shc.handle_proposal(context, proposal_init, content_receiver).await
                    } else {
                        Ok(ShcReturn::Tasks(Vec::new()))
                    }
                }
                None => {
                    trace!("Drop proposal from just completed height. {:?}", proposal_init);
                    Ok(ShcReturn::Tasks(Vec::new()))
                }
            },
        }
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
        match message.height.cmp(&height.0) {
            std::cmp::Ordering::Greater => {
                if self.should_cache_vote(&height, 0, &message) {
                    trace!("Cache message for a future height. {:?}", message);
                    self.future_votes.entry(message.height).or_default().push(message);
                }
                Ok(ShcReturn::Tasks(Vec::new()))
            }
            std::cmp::Ordering::Less => {
                trace!("Drop message from past height. {:?}", message);
                Ok(ShcReturn::Tasks(Vec::new()))
            }
            std::cmp::Ordering::Equal => match shc {
                Some(shc) => {
                    if self.should_cache_vote(&height, shc.current_round(), &message) {
                        shc.handle_vote(context, message).await
                    } else {
                        Ok(ShcReturn::Tasks(Vec::new()))
                    }
                }
                None => {
                    trace!("Drop message from just completed height. {:?}", message);
                    Ok(ShcReturn::Tasks(Vec::new()))
                }
            },
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
            let Some(entry) = self.cached_proposals.first_entry() else {
                return Vec::new();
            };
            match entry.key().cmp(&height.0) {
                std::cmp::Ordering::Greater => return vec![],
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

    /// Filters the cached messages:
    /// - returns (and removes from stored votes) all of the current height votes.
    /// - drops votes from earlier heights.
    /// - retains future votes in the cache.
    fn get_current_height_votes(&mut self, height: BlockNumber) -> Vec<Vote> {
        // Depends on `future_votes` being sorted by height.
        loop {
            let Some(entry) = self.future_votes.first_entry() else {
                return Vec::new();
            };
            match entry.key().cmp(&height.0) {
                std::cmp::Ordering::Greater => return Vec::new(),
                std::cmp::Ordering::Equal => return entry.remove(),
                std::cmp::Ordering::Less => {
                    entry.remove();
                }
            }
        }
    }

    fn report_max_cached_block_number_metric(&self, height: BlockNumber) {
        // If nothing is cached use current height as "max".
        let max_cached_block_number = self.cached_proposals.keys().max().unwrap_or(&height.0);
        CONSENSUS_MAX_CACHED_BLOCK_NUMBER.set_lossy(*max_cached_block_number);
    }

    fn should_cache_msg(
        &self,
        current_height: &BlockNumber,
        current_round: u32,
        msg_height: u64,
        msg_round: u32,
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
                current_height.0,
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
        current_round: u32,
        proposal: &ProposalInit,
    ) -> bool {
        self.should_cache_msg(
            current_height,
            current_round,
            proposal.height.0,
            proposal.round,
            "proposal",
        )
    }

    fn should_cache_vote(
        &self,
        current_height: &BlockNumber,
        current_round: u32,
        vote: &Vote,
    ) -> bool {
        self.should_cache_msg(current_height, current_round, vote.height, vote.round, "vote")
    }
}
