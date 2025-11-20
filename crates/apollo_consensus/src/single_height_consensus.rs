//! Run a single height of consensus.
//!
//! [`SingleHeightConsensus`] (SHC) - run consensus for a single height.
//!
//! [`ShcTask`] - a task which should be run without blocking consensus.

#[cfg(test)]
#[path = "single_height_consensus_test.rs"]
mod single_height_consensus_test;

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const REBROADCAST_LOG_PERIOD_SECS: u64 = 10;

use apollo_consensus_config::config::TimeoutsConfig;
use apollo_infra_utils::trace_every_n_sec;
use apollo_protobuf::consensus::{ProposalInit, Vote, VoteType};
#[cfg(test)]
use enum_as_inner::EnumAsInner;
use futures::channel::{mpsc, oneshot};
use starknet_api::block::BlockNumber;
use tracing::{debug, info, instrument, trace, warn};

use crate::metrics::{
    CONSENSUS_BUILD_PROPOSAL_FAILED,
    CONSENSUS_BUILD_PROPOSAL_TOTAL,
    CONSENSUS_CONFLICTING_VOTES,
    CONSENSUS_PROPOSALS_INVALID,
    CONSENSUS_PROPOSALS_VALIDATED,
    CONSENSUS_PROPOSALS_VALID_INIT,
    CONSENSUS_REPROPOSALS,
};
use crate::state_machine::{SMRequest, StateMachine, StateMachineEvent};
use crate::storage::HeightVotedStorageTrait;
use crate::types::{
    ConsensusContext,
    ConsensusError,
    Decision,
    ProposalCommitment,
    Round,
    ValidatorId,
};
use crate::votes_threshold::QuorumType;

/// The SHC can either update the manager of a decision or return tasks that should be run without
/// blocking further calls to itself.
#[derive(Debug, PartialEq)]
#[cfg_attr(test, derive(EnumAsInner))]
pub(crate) enum ShcReturn {
    Tasks(Vec<ShcTask>),
    Decision(Decision),
}

/// A task which should be run without blocking calls to SHC.
#[derive(Debug)]
#[cfg_attr(test, derive(EnumAsInner))]
pub(crate) enum ShcTask {
    TimeoutPropose(Duration, StateMachineEvent),
    TimeoutPrevote(Duration, StateMachineEvent),
    TimeoutPrecommit(Duration, StateMachineEvent),
    /// Periodic rebroadcast of the latest self vote of the given type.
    Rebroadcast(Duration, Vote),
    /// Building a proposal is handled in 3 stages:
    /// 1. The SHC requests a block to be built from the context.
    /// 2. SHC returns, allowing the context to build the block while the Manager awaits the result
    ///    without blocking consensus.
    /// 3. Once building is complete, the manager returns the built block to the SHC as an event,
    ///    which can be sent to the SM.
    /// * During this process, the SM is frozen; it will accept and buffer other events, only
    ///   processing them once it receives the built proposal.
    BuildProposal(Round, oneshot::Receiver<ProposalCommitment>),
    /// Validating a proposal is handled in 3 stages:
    /// 1. The SHC validates `ProposalInit`, then starts block validation within the context.
    /// 2. SHC returns, allowing the context to validate the content while the Manager awaits the
    ///    result without blocking consensus.
    /// 3. Once validation is complete, the manager returns the built proposal to the SHC as an
    ///    event, which can be sent to the SM.
    ValidateProposal(ProposalInit, oneshot::Receiver<ProposalCommitment>),
}

impl PartialEq for ShcTask {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ShcTask::TimeoutPropose(d1, e1), ShcTask::TimeoutPropose(d2, e2))
            | (ShcTask::TimeoutPrevote(d1, e1), ShcTask::TimeoutPrevote(d2, e2))
            | (ShcTask::TimeoutPrecommit(d1, e1), ShcTask::TimeoutPrecommit(d2, e2)) => {
                d1 == d2 && e1 == e2
            }
            (ShcTask::Rebroadcast(d1, v1), ShcTask::Rebroadcast(d2, v2)) => d1 == d2 && v1 == v2,
            (ShcTask::BuildProposal(r1, _), ShcTask::BuildProposal(r2, _)) => r1 == r2,
            (ShcTask::ValidateProposal(pi1, _), ShcTask::ValidateProposal(pi2, _)) => pi1 == pi2,
            _ => false,
        }
    }
}

impl ShcTask {
    pub(crate) async fn run(self) -> StateMachineEvent {
        trace!("Running task: {:?}", self);
        match self {
            ShcTask::TimeoutPropose(duration, event)
            | ShcTask::TimeoutPrevote(duration, event)
            | ShcTask::TimeoutPrecommit(duration, event) => {
                tokio::time::sleep(duration).await;
                event
            }
            ShcTask::Rebroadcast(duration, vote) => {
                tokio::time::sleep(duration).await;
                StateMachineEvent::RebroadcastVote(vote)
            }
            ShcTask::BuildProposal(round, receiver) => {
                let proposal_id = receiver.await.ok();
                StateMachineEvent::FinishedBuilding(proposal_id, round)
            }
            ShcTask::ValidateProposal(init, block_receiver) => {
                // TODO(Asmaa): Consider if we want to differentiate between an interrupt and other
                // failures.
                let proposal_id = block_receiver.await.ok();
                StateMachineEvent::FinishedValidation(proposal_id, init.round, init.valid_round)
            }
        }
    }
}

/// Represents a single height of consensus. It is responsible for mapping between the idealized
/// view of consensus represented in the StateMachine and the real world implementation.
///
/// Example:
/// - Timeouts: the SM returns an event timeout, but SHC then maps that to a task which can be run
///   by the Manager. The manager though unaware of the specific task as it has minimal consensus
///   logic.
///
/// Each height is begun with a call to `start`, with no further calls to it.
///
/// SHC is not a top level task, it is called directly and returns values (doesn't directly run sub
/// tasks). SHC does have side effects, such as sending messages to the network via the context.
pub(crate) struct SingleHeightConsensus {
    validators: Vec<ValidatorId>,
    timeouts: TimeoutsConfig,
    state_machine: StateMachine,
    // Tracks rounds for which we started validating a proposal to avoid duplicate validations.
    pending_validation_rounds: HashSet<Round>,
    height_voted_storage: Arc<Mutex<dyn HeightVotedStorageTrait>>,
}

impl SingleHeightConsensus {
    pub(crate) fn new(
        height: BlockNumber,
        is_observer: bool,
        id: ValidatorId,
        validators: Vec<ValidatorId>,
        quorum_type: QuorumType,
        timeouts: TimeoutsConfig,
        height_voted_storage: Arc<Mutex<dyn HeightVotedStorageTrait>>,
    ) -> Self {
        // TODO(matan): Use actual weights, not just `len`.
        let n_validators =
            u64::try_from(validators.len()).expect("Should have way less than u64::MAX validators");
        let state_machine = StateMachine::new(height, id, n_validators, is_observer, quorum_type);
        Self {
            validators,
            timeouts,
            state_machine,
            pending_validation_rounds: HashSet::new(),
            height_voted_storage,
        }
    }

    pub(crate) fn current_round(&self) -> Round {
        self.state_machine.round()
    }

    #[instrument(skip_all)]
    pub(crate) async fn start<ContextT: ConsensusContext>(
        &mut self,
        context: &mut ContextT,
    ) -> Result<ShcReturn, ConsensusError> {
        let height = self.state_machine.height();
        context.set_height_and_round(height, self.state_machine.round()).await;
        let leader_fn = |round: Round| -> ValidatorId { context.proposer(height, round) };
        let requests = self.state_machine.start(&leader_fn);
        let ret = self.handle_state_machine_requests(context, requests).await;
        // Defensive programming. We don't expect the height and round to have changed from the
        // start of this method.
        context.set_height_and_round(height, self.state_machine.round()).await;
        ret
    }

    /// Process the proposal init and initiate block validation. See [`ShcTask::ValidateProposal`]
    /// for more details on the full proposal flow.
    #[instrument(skip_all)]
    pub(crate) async fn handle_proposal<ContextT: ConsensusContext>(
        &mut self,
        context: &mut ContextT,
        init: ProposalInit,
        p2p_messages_receiver: mpsc::Receiver<ContextT::ProposalPart>,
    ) -> Result<ShcReturn, ConsensusError> {
        debug!("Received {init:?}");
        let height = self.state_machine.height();
        if init.height != height {
            warn!("Invalid proposal height: expected {:?}, got {:?}", height, init.height);
            return Ok(ShcReturn::Tasks(Vec::new()));
        }
        let proposer_id = context.proposer(height, init.round);
        if init.proposer != proposer_id {
            warn!("Invalid proposer: expected {:?}, got {:?}", proposer_id, init.proposer);
            return Ok(ShcReturn::Tasks(Vec::new()));
        }
        // Avoid duplicate validations:
        // - If SM already has an entry for this round, a (re)proposal was already recorded.
        // - If we already started validating this round, ignore repeats.
        if self.state_machine.has_proposal_for_round(init.round)
            || self.pending_validation_rounds.contains(&init.round)
        {
            warn!("Round {} already handled a proposal, ignoring", init.round);
            return Ok(ShcReturn::Tasks(Vec::new()));
        }
        let timeout = self.timeouts.get_proposal_timeout(init.round);
        info!(
            "Accepting {init:?}. node_round: {}, timeout: {timeout:?}",
            self.state_machine.round()
        );
        CONSENSUS_PROPOSALS_VALID_INIT.increment(1);

        // Since validating the proposal is non-blocking, avoid validating the same round twice in
        // parallel (e.g., due to repeats or spam).
        self.pending_validation_rounds.insert(init.round);
        let block_receiver = context.validate_proposal(init, timeout, p2p_messages_receiver).await;
        context.set_height_and_round(height, self.state_machine.round()).await;
        Ok(ShcReturn::Tasks(vec![ShcTask::ValidateProposal(init, block_receiver)]))
    }

    #[instrument(skip_all)]
    pub(crate) async fn handle_event<ContextT: ConsensusContext>(
        &mut self,
        context: &mut ContextT,
        event: StateMachineEvent,
    ) -> Result<ShcReturn, ConsensusError> {
        trace!("Received StateMachineEvent: {:?}", event);
        let height = self.state_machine.height();
        let leader_fn = |round: Round| -> ValidatorId { context.proposer(height, round) };
        let ret = match event {
            StateMachineEvent::TimeoutPropose(_round)
            | StateMachineEvent::TimeoutPrevote(_round)
            | StateMachineEvent::TimeoutPrecommit(_round) => {
                self.handle_timeout(context, event).await
            }
            StateMachineEvent::RebroadcastVote(vote) => match vote.vote_type {
                VoteType::Prevote => {
                    let Some(last_vote) = self.state_machine.last_self_prevote() else {
                        return Err(ConsensusError::InternalInconsistency(
                            "No prevote to send".to_string(),
                        ));
                    };
                    if last_vote.round > vote.round {
                        // Only replay the newest prevote.
                        return Ok(ShcReturn::Tasks(Vec::new()));
                    }
                    trace_every_n_sec!(REBROADCAST_LOG_PERIOD_SECS, "Rebroadcasting {last_vote:?}");
                    context.broadcast(last_vote.clone()).await?;
                    Ok(ShcReturn::Tasks(vec![ShcTask::Rebroadcast(
                        self.timeouts.get_prevote_timeout(0),
                        vote,
                    )]))
                }
                VoteType::Precommit => {
                    let Some(last_vote) = self.state_machine.last_self_precommit() else {
                        return Err(ConsensusError::InternalInconsistency(
                            "No precommit to send".to_string(),
                        ));
                    };
                    if last_vote.round > vote.round {
                        // Only replay the newest precommit.
                        return Ok(ShcReturn::Tasks(Vec::new()));
                    }
                    trace_every_n_sec!(REBROADCAST_LOG_PERIOD_SECS, "Rebroadcasting {last_vote:?}");
                    context.broadcast(last_vote.clone()).await?;
                    Ok(ShcReturn::Tasks(vec![ShcTask::Rebroadcast(
                        self.timeouts.get_precommit_timeout(0),
                        vote,
                    )]))
                }
            },
            StateMachineEvent::FinishedValidation(proposal_id, round, valid_round) => {
                let height = self.state_machine.height();
                let leader_fn = |round: Round| -> ValidatorId { context.proposer(height, round) };
                debug!(
                    proposer = %leader_fn(round),
                    %round,
                    ?valid_round,
                    proposal_commitment = ?proposal_id,
                    node_round = self.state_machine.round(),
                    "Validated proposal.",
                );
                if proposal_id.is_some() {
                    CONSENSUS_PROPOSALS_VALIDATED.increment(1);
                } else {
                    CONSENSUS_PROPOSALS_INVALID.increment(1);
                }

                // Cleanup: validation for round {round} finished, so remove it from the pending
                // set. This doesn't affect logic.
                self.pending_validation_rounds.remove(&round);
                let requests = self.state_machine.handle_event(event, &leader_fn);
                self.handle_state_machine_requests(context, requests).await
            }
            StateMachineEvent::FinishedBuilding(proposal_id, round) => {
                if proposal_id.is_none() {
                    CONSENSUS_BUILD_PROPOSAL_FAILED.increment(1);
                }
                // Ensure SM has no proposal recorded yet for this round when proposing.
                assert!(
                    !self.state_machine.has_proposal_for_round(round),
                    "There should be no entry for round {round} when proposing"
                );

                assert_eq!(
                    round,
                    self.state_machine.round(),
                    "State machine should not progress while awaiting proposal"
                );
                debug!(%round, proposal_commitment = ?proposal_id, "Built proposal.");
                let requests = self.state_machine.handle_event(event, &leader_fn);
                self.handle_state_machine_requests(context, requests).await
            }
            StateMachineEvent::Prevote(_) | StateMachineEvent::Precommit(_) => {
                unreachable!("Peer votes must be handled via handle_vote")
            }
        };
        context.set_height_and_round(self.state_machine.height(), self.state_machine.round()).await;
        ret
    }

    async fn handle_timeout<ContextT: ConsensusContext>(
        &mut self,
        context: &mut ContextT,
        event: StateMachineEvent,
    ) -> Result<ShcReturn, ConsensusError> {
        let height = self.state_machine.height();
        let leader_fn = |round: Round| -> ValidatorId { context.proposer(height, round) };
        let sm_requests = self.state_machine.handle_event(event, &leader_fn);
        self.handle_state_machine_requests(context, sm_requests).await
    }

    /// Handle vote messages from peer nodes.
    #[instrument(skip_all)]
    pub(crate) async fn handle_vote<ContextT: ConsensusContext>(
        &mut self,
        context: &mut ContextT,
        vote: Vote,
    ) -> Result<ShcReturn, ConsensusError> {
        trace!("Received {:?}", vote);
        if !self.validators.contains(&vote.voter) {
            debug!("Ignoring vote from non validator: vote={:?}", vote);
            return Ok(ShcReturn::Tasks(Vec::new()));
        }

        // Check duplicates/conflicts from SM stored votes.
        let (votes_map, sm_vote) = match vote.vote_type {
            VoteType::Prevote => {
                (self.state_machine.prevotes_ref(), StateMachineEvent::Prevote(vote.clone()))
            }
            VoteType::Precommit => {
                (self.state_machine.precommits_ref(), StateMachineEvent::Precommit(vote.clone()))
            }
        };
        if let Some((old_vote, _)) = votes_map.get(&(vote.round, vote.voter)) {
            if old_vote.proposal_commitment == vote.proposal_commitment {
                // Duplicate - ignore.
                return Ok(ShcReturn::Tasks(Vec::new()));
            } else {
                // Conflict - ignore and record.
                warn!("Conflicting votes: old={old_vote:?}, new={vote:?}");
                CONSENSUS_CONFLICTING_VOTES.increment(1);
                return Ok(ShcReturn::Tasks(Vec::new()));
            }
        }

        info!("Accepting {:?}", vote);
        let height = self.state_machine.height();
        let leader_fn = |round: Round| -> ValidatorId { context.proposer(height, round) };
        // TODO(Asmaa): consider calling handle_prevote/precommit instead of sending the vote event.
        let requests = self.state_machine.handle_event(sm_vote, &leader_fn);
        let ret = self.handle_state_machine_requests(context, requests).await;
        context.set_height_and_round(height, self.state_machine.round()).await;
        ret
    }

    // Handle requests output by the state machine.
    async fn handle_state_machine_requests<ContextT: ConsensusContext>(
        &mut self,
        context: &mut ContextT,
        mut requests: VecDeque<SMRequest>,
    ) -> Result<ShcReturn, ConsensusError> {
        let mut ret_val = Vec::new();
        while let Some(request) = requests.pop_front() {
            trace!("Handling sm request: {:?}", request);
            match request {
                SMRequest::StartBuildProposal(round) => {
                    // Ensure SM has no proposal recorded yet for this round when proposing.
                    assert!(
                        !self.state_machine.has_proposal_for_round(round),
                        "There should be no entry for round {round} when proposing"
                    );
                    // TODO(Matan): Figure out how to handle failed proposal building. I believe
                    // this should be handled by applying timeoutPropose when we
                    // are the leader.
                    let init = ProposalInit {
                        height: self.state_machine.height(),
                        round,
                        proposer: self.state_machine.validator_id(),
                        valid_round: None,
                    };
                    CONSENSUS_BUILD_PROPOSAL_TOTAL.increment(1);
                    // TODO(Asmaa): Reconsider: we should keep the builder's timeout bounded
                    // independently of the consensus proposal timeout. We currently use the base
                    // (round 0) proposal timeout for building to avoid giving the Batcher more time
                    // when proposal time is extended for consensus.
                    let fin_receiver =
                        context.build_proposal(init, self.timeouts.get_proposal_timeout(0)).await;
                    ret_val.push(ShcTask::BuildProposal(round, fin_receiver));
                }
                SMRequest::BroadcastVote(vote) => {
                    let rebroadcast_task = match vote.vote_type {
                        VoteType::Prevote => {
                            ShcTask::Rebroadcast(self.timeouts.get_prevote_timeout(0), vote.clone())
                        }
                        VoteType::Precommit => ShcTask::Rebroadcast(
                            self.timeouts.get_precommit_timeout(0),
                            vote.clone(),
                        ),
                    };
                    // Ensure the voter matches this node.
                    assert_eq!(vote.voter, self.state_machine.validator_id());
                    trace!("Writing voted height {} to storage", self.state_machine.height());
                    self.height_voted_storage
                        .lock()
                        .expect(
                            "Lock should never be poisoned because there should never be \
                             concurrent access.",
                        )
                        .set_prev_voted_height(self.state_machine.height())
                        .expect("Failed to write voted height {self.height} to storage");

                    info!("Broadcasting {vote:?}");
                    context.broadcast(vote).await?;
                    ret_val.push(rebroadcast_task);
                }
                SMRequest::ScheduleTimeoutPropose(round) => {
                    ret_val.push(ShcTask::TimeoutPropose(
                        self.timeouts.get_proposal_timeout(round),
                        StateMachineEvent::TimeoutPropose(round),
                    ));
                }
                SMRequest::ScheduleTimeoutPrevote(round) => {
                    ret_val.push(ShcTask::TimeoutPrevote(
                        self.timeouts.get_prevote_timeout(round),
                        StateMachineEvent::TimeoutPrevote(round),
                    ));
                }
                SMRequest::ScheduleTimeoutPrecommit(round) => {
                    ret_val.push(ShcTask::TimeoutPrecommit(
                        self.timeouts.get_precommit_timeout(round),
                        StateMachineEvent::TimeoutPrecommit(round),
                    ));
                }
                SMRequest::DecisionReached(proposal_id, round) => {
                    return self.handle_state_machine_decision(proposal_id, round).await;
                }
                SMRequest::Repropose(proposal_id, init) => {
                    // Make sure there is an existing proposal for the valid round and it matches
                    // the proposal ID.
                    let Some(valid_round) = init.valid_round else {
                        // Newly built proposals are handled by the BuildProposal flow.
                        continue;
                    };
                    let existing = self.state_machine.proposal_id_for_round(valid_round);
                    assert!(
                        existing.is_some_and(|id| id == proposal_id),
                        "A proposal with ID {proposal_id:?} should exist for valid_round: \
                         {valid_round}. Found: {existing:?}",
                    );
                    CONSENSUS_REPROPOSALS.increment(1);
                    context.repropose(proposal_id, init).await;
                }
                SMRequest::ScheduleTimeoutRebroadcast(_) => {
                    unimplemented!("ScheduleTimeoutRebroadcast is not supported.")
                }
                SMRequest::StartValidateProposal(_) => {
                    unimplemented!("StartValidateProposal is not supported.")
                }
            }
        }
        Ok(ShcReturn::Tasks(ret_val))
    }

    async fn handle_state_machine_decision(
        &mut self,
        proposal_id: ProposalCommitment,
        round: Round,
    ) -> Result<ShcReturn, ConsensusError> {
        let invalid_decision = |msg: String| {
            ConsensusError::InternalInconsistency(format!(
                "Invalid decision: sm_proposal_id={proposal_id}, round={round}. {msg}",
            ))
        };
        let block = self
            .state_machine
            .proposal_id_for_round(round)
            .ok_or_else(|| invalid_decision("No proposal entry for this round".to_string()))?;
        if block != proposal_id {
            return Err(invalid_decision(format!(
                "StateMachine proposal commitment should match the stored block. Shc.block_id: \
                 {block}"
            )));
        }
        let supporting_precommits = self.precommit_votes_for_value(round, Some(proposal_id));

        // TODO(matan): Check actual weights.
        let vote_weight = u64::try_from(supporting_precommits.len())
            .expect("Should have way less than u64::MAX supporting votes");
        let total_weight = self.state_machine.total_weight();

        if !self.state_machine.quorum().is_met(vote_weight, total_weight) {
            let msg = format!(
                "Not enough supporting votes. num_supporting_votes: {vote_weight} out of \
                 {total_weight}. supporting_votes: {supporting_precommits:?}",
            );
            return Err(invalid_decision(msg));
        }
        Ok(ShcReturn::Decision(Decision { precommits: supporting_precommits, block }))
    }

    fn precommit_votes_for_value(
        &self,
        round: Round,
        value: Option<ProposalCommitment>,
    ) -> Vec<Vote> {
        self.state_machine
            .precommits_ref()
            .iter()
            .filter_map(|(&(r, _voter), (v, _w))| {
                if r == round && v.proposal_commitment == value { Some(v.clone()) } else { None }
            })
            .collect()
    }
}
