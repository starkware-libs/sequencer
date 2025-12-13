//! Run a single height of consensus.
//!
//! [`SingleHeightConsensus`] (SHC) - run consensus for a single height.
//!
//! SHC returns SMRequests to be executed by the manager; it does not run tasks itself.

#[cfg(test)]
#[path = "single_height_consensus_test.rs"]
mod single_height_consensus_test;

use std::collections::{HashSet, VecDeque};

use crate::state_machine::VoteStatus;
const REBROADCAST_LOG_PERIOD_SECS: u64 = 10;
const DUPLICATE_VOTE_LOG_PERIOD_SECS: u64 = 10;

use apollo_consensus_config::config::TimeoutsConfig;
use apollo_infra_utils::trace_every_n_sec;
use apollo_protobuf::consensus::{ProposalInit, Vote, VoteType};
#[cfg(test)]
use enum_as_inner::EnumAsInner;
use starknet_api::block::BlockNumber;
use tracing::{debug, info, instrument, trace, warn};

use crate::metrics::{
    CONSENSUS_BUILD_PROPOSAL_FAILED,
    CONSENSUS_BUILD_PROPOSAL_TOTAL,
    CONSENSUS_CONFLICTING_VOTES,
    CONSENSUS_PROPOSALS_INVALID,
    CONSENSUS_PROPOSALS_VALIDATED,
    CONSENSUS_PROPOSALS_VALID_INIT,
};
use crate::state_machine::{SMRequest, StateMachine, StateMachineEvent};
use crate::types::{ConsensusError, Decision, ProposalCommitment, Round, ValidatorId};
use crate::votes_threshold::QuorumType;

/// The SHC can either update the manager of a decision or return requests for the manager to run,
/// without blocking further calls to itself.
#[derive(Debug, PartialEq)]
#[cfg_attr(test, derive(EnumAsInner))]
pub(crate) enum ShcReturn {
    Requests(VecDeque<SMRequest>),
    Decision(Decision),
}

/// Represents a single height of consensus. It is responsible for mapping between the idealized
/// view of consensus represented in the StateMachine and the real world implementation.
///
/// Example:
/// - Timeouts: the SM returns schedule requests; SHC returns these as SMRequests for the Manager to
///   run (e.g., schedule timeouts, broadcast, etc.). The manager has minimal consensus logic.
///
/// Each height is begun with a call to `start`, with no further calls to it.
///
/// SHC is not a top level task; it is called directly and returns values (does not run sub-tasks).
/// SHC has no timers and does not perform IO; the manager executes requests and returns
/// `StateMachineEvent`s back to SHC.
pub(crate) struct SingleHeightConsensus {
    validators: Vec<ValidatorId>,
    timeouts: TimeoutsConfig,
    state_machine: StateMachine,
    // Tracks rounds for which we started validating a proposal to avoid duplicate validations.
    pending_validation_rounds: HashSet<Round>,
}

impl SingleHeightConsensus {
    pub(crate) fn new(
        height: BlockNumber,
        is_observer: bool,
        id: ValidatorId,
        validators: Vec<ValidatorId>,
        quorum_type: QuorumType,
        timeouts: TimeoutsConfig,
    ) -> Self {
        // TODO(matan): Use actual weights, not just `len`.
        let n_validators =
            u64::try_from(validators.len()).expect("Should have way less than u64::MAX validators");
        let state_machine = StateMachine::new(height, id, n_validators, is_observer, quorum_type);
        Self { validators, timeouts, state_machine, pending_validation_rounds: HashSet::new() }
    }

    pub(crate) fn current_round(&self) -> Round {
        self.state_machine.round()
    }

    #[instrument(skip_all)]
    pub(crate) fn start<LeaderFn>(
        &mut self,
        leader_fn: &LeaderFn,
    ) -> Result<ShcReturn, ConsensusError>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        let requests = self.state_machine.start(leader_fn);
        self.handle_state_machine_requests(requests)
    }

    /// Process the proposal init and initiate block validation by returning
    /// `SMRequest::StartValidateProposal` to the manager.
    #[instrument(skip_all)]
    pub(crate) fn handle_proposal<LeaderFn>(
        &mut self,
        leader_fn: &LeaderFn,
        init: ProposalInit,
    ) -> Result<ShcReturn, ConsensusError>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        debug!("Received {init:?}");
        let height = self.state_machine.height();
        if init.height != height {
            warn!("Invalid proposal height: expected {:?}, got {:?}", height, init.height);
            return Ok(ShcReturn::Requests(VecDeque::new()));
        }
        let proposer_id = leader_fn(init.round);
        if init.proposer != proposer_id {
            warn!("Invalid proposer: expected {:?}, got {:?}", proposer_id, init.proposer);
            return Ok(ShcReturn::Requests(VecDeque::new()));
        }
        // Avoid duplicate validations:
        // - If SM already has an entry for this round, a (re)proposal was already recorded.
        // - If we already started validating this round, ignore repeats.
        if self.state_machine.has_proposal_for_round(init.round)
            || self.pending_validation_rounds.contains(&init.round)
        {
            warn!("Round {} already handled a proposal, ignoring", init.round);
            return Ok(ShcReturn::Requests(VecDeque::new()));
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
        // Ask the manager to start validation.
        Ok(ShcReturn::Requests(VecDeque::from([SMRequest::StartValidateProposal(init)])))
    }

    #[instrument(skip_all)]
    pub(crate) fn handle_event<LeaderFn>(
        &mut self,
        leader_fn: &LeaderFn,
        event: StateMachineEvent,
    ) -> Result<ShcReturn, ConsensusError>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        trace!("Received StateMachineEvent: {:?}", event);
        match event {
            StateMachineEvent::TimeoutPropose(_)
            | StateMachineEvent::TimeoutPrevote(_)
            | StateMachineEvent::TimeoutPrecommit(_) => self.handle_timeout_event(leader_fn, event),
            StateMachineEvent::VoteBroadcasted(vote) => self.handle_vote_broadcasted(vote),
            StateMachineEvent::FinishedValidation(proposal_id, round, valid_round) => {
                self.handle_finished_validation(leader_fn, proposal_id, round, valid_round)
            }
            StateMachineEvent::FinishedBuilding(proposal_id, round) => {
                self.handle_finished_building(leader_fn, proposal_id, round)
            }
            StateMachineEvent::Prevote(_) | StateMachineEvent::Precommit(_) => {
                unreachable!("Peer votes must be handled via handle_vote")
            }
        }
    }

    fn handle_timeout_event<LeaderFn>(
        &mut self,
        leader_fn: &LeaderFn,
        event: StateMachineEvent,
    ) -> Result<ShcReturn, ConsensusError>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        let sm_requests = self.state_machine.handle_event(event, leader_fn);
        self.handle_state_machine_requests(sm_requests)
    }

    fn handle_vote_broadcasted(&mut self, vote: Vote) -> Result<ShcReturn, ConsensusError> {
        let last_vote = match vote.vote_type {
            VoteType::Prevote => self.state_machine.last_self_prevote().ok_or_else(|| {
                ConsensusError::InternalInconsistency("No prevote to send".to_string())
            })?,
            VoteType::Precommit => self.state_machine.last_self_precommit().ok_or_else(|| {
                ConsensusError::InternalInconsistency("No precommit to send".to_string())
            })?,
        };
        if last_vote.round > vote.round {
            // Only rebroadcast the newest vote.
            return Ok(ShcReturn::Requests(VecDeque::new()));
        }
        assert_eq!(last_vote, vote);
        trace_every_n_sec!(REBROADCAST_LOG_PERIOD_SECS, "Rebroadcasting {last_vote:?}");
        Ok(ShcReturn::Requests(VecDeque::from([SMRequest::BroadcastVote(last_vote)])))
    }

    fn handle_finished_validation<LeaderFn>(
        &mut self,
        leader_fn: &LeaderFn,
        proposal_id: Option<ProposalCommitment>,
        round: Round,
        valid_round: Option<Round>,
    ) -> Result<ShcReturn, ConsensusError>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
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
        let requests = self.state_machine.handle_event(
            StateMachineEvent::FinishedValidation(proposal_id, round, None),
            leader_fn,
        );
        self.handle_state_machine_requests(requests)
    }

    fn handle_finished_building<LeaderFn>(
        &mut self,
        leader_fn: &LeaderFn,
        proposal_id: Option<ProposalCommitment>,
        round: Round,
    ) -> Result<ShcReturn, ConsensusError>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        if proposal_id.is_none() {
            CONSENSUS_BUILD_PROPOSAL_FAILED.increment(1);
        }
        CONSENSUS_BUILD_PROPOSAL_TOTAL.increment(1);
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
        let requests = self
            .state_machine
            .handle_event(StateMachineEvent::FinishedBuilding(proposal_id, round), leader_fn);
        self.handle_state_machine_requests(requests)
    }

    /// Handle vote messages from peer nodes.
    #[instrument(skip_all)]
    pub(crate) fn handle_vote<LeaderFn>(
        &mut self,
        leader_fn: &LeaderFn,
        vote: Vote,
    ) -> Result<ShcReturn, ConsensusError>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        trace!("Received {:?}", vote);
        if !self.validators.contains(&vote.voter) {
            debug!("Ignoring vote from non validator: vote={:?}", vote);
            return Ok(ShcReturn::Requests(VecDeque::new()));
        }

        // Check if vote has already been received.
        match self.state_machine.received_vote(&vote) {
            VoteStatus::Duplicate => {
                // Duplicate - ignore.
                trace_every_n_sec!(
                    DUPLICATE_VOTE_LOG_PERIOD_SECS,
                    "Ignoring duplicate vote: {vote:?}"
                );
                return Ok(ShcReturn::Requests(VecDeque::new()));
            }
            VoteStatus::Conflict(old_vote, new_vote) => {
                // Conflict - ignore and record.
                warn!("Conflicting votes: old={old_vote:?}, new={new_vote:?}");
                CONSENSUS_CONFLICTING_VOTES.increment(1);
                return Ok(ShcReturn::Requests(VecDeque::new()));
            }
            VoteStatus::New => {
                // Vote is new, proceed to process it.
            }
        }

        info!("Accepting {:?}", vote);
        let sm_vote = match vote.vote_type {
            VoteType::Prevote => StateMachineEvent::Prevote(vote),
            VoteType::Precommit => StateMachineEvent::Precommit(vote),
        };
        let requests = self.state_machine.handle_event(sm_vote, leader_fn);
        self.handle_state_machine_requests(requests)
    }

    // Handle requests output by the state machine.
    fn handle_state_machine_requests(
        &mut self,
        requests: VecDeque<SMRequest>,
    ) -> Result<ShcReturn, ConsensusError> {
        // If any request indicates a decision, handle it immediately regardless of position.
        if let Some(&SMRequest::DecisionReached(proposal_id, round)) =
            requests.iter().find(|r| matches!(r, SMRequest::DecisionReached(_, _)))
        {
            return self.handle_state_machine_decision(proposal_id, round);
        }
        Ok(ShcReturn::Requests(requests))
    }

    fn handle_state_machine_decision(
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
