//! State machine for Starknet consensus.
//!
//! LOC refers to the line of code from Algorithm 1 (page 6) of the tendermint
//! [paper](https://arxiv.org/pdf/1807.04938).

#[cfg(test)]
#[path = "state_machine_test.rs"]
mod state_machine_test;

use std::collections::{HashMap, HashSet, VecDeque};

use apollo_protobuf::consensus::{ProposalInit, Vote, VoteType};
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use tracing::{debug, info, trace, warn};

use crate::metrics::{
    TimeoutType,
    CONSENSUS_HELD_LOCKS,
    CONSENSUS_NEW_VALUE_LOCKS,
    CONSENSUS_ROUND,
    CONSENSUS_ROUND_ABOVE_ZERO,
    CONSENSUS_ROUND_ADVANCES,
    CONSENSUS_TIMEOUTS,
    LABEL_NAME_TIMEOUT_TYPE,
};
use crate::types::{ProposalCommitment, Round, ValidatorId};
use crate::votes_threshold::{QuorumType, VotesThreshold, ROUND_SKIP_THRESHOLD};

/// The unique identifier for a specific validator's vote in a specific round.
type VoteKey = (Round, ValidatorId);

/// A vote accompanied by the validator's voting weight.
type WeightedVote = (Vote, u32);

/// A map of votes, keyed by round and validator ID, with the vote and its weight.
type VotesMap = HashMap<VoteKey, WeightedVote>;

/// Events which the state machine receives. These represent completion events
/// fed back to the SM after an external task is done.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StateMachineEvent {
    /// The local proposal building task has completed.
    FinishedBuilding(Option<ProposalCommitment>, Round),
    /// A proposal validation task has completed. (proposal_id, round, valid_round)
    FinishedValidation(Option<ProposalCommitment>, Round, Option<Round>),
    /// Prevote message, sent from the SHC to the state machine.
    Prevote(Vote),
    /// Precommit message, sent from the SHC to the state machine.
    Precommit(Vote),
    /// TimeoutPropose event, sent from the SHC to the state machine.
    TimeoutPropose(Round),
    /// TimeoutPrevote event, sent from the SHC to the state machine.
    TimeoutPrevote(Round),
    /// TimeoutPrecommit event, sent from the SHC to the state machine.
    TimeoutPrecommit(Round),
    /// Used by the manager to notify the SHC that a vote has been broadcast, for rebroadcasting.
    VoteBroadcasted(Vote),
}

/// Requests the SM/SHC sends to the caller for execution.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SMRequest {
    /// Request to build a proposal for a new round.
    StartBuildProposal(Round),
    /// Request to validate a received proposal from the network.
    StartValidateProposal(ProposalInit),
    /// Request to broadcast a Prevote or Precommit vote.
    BroadcastVote(Vote),
    /// Request to schedule a TimeoutPropose.
    ScheduleTimeoutPropose(Round),
    /// Request to schedule a TimeoutPrevote.
    ScheduleTimeoutPrevote(Round),
    /// Request to schedule a TimeoutPrecommit.
    ScheduleTimeoutPrecommit(Round),
    /// Decision reached for the given proposal and round.
    DecisionReached(ProposalCommitment, Round),
    /// Request to re-propose (sent by the leader after advancing to a new round
    /// with a locked/valid value).
    Repropose(ProposalCommitment, ProposalInit),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) enum Step {
    Propose,
    Prevote,
    Precommit,
}

/// State Machine. Major assumptions:
/// 1. SHC handles: authentication, replays, and conflicts.
/// 2. SM must handle "out of order" messages (E.g. vote arrives before proposal).
///
/// Each height is begun with a call to `start`, with no further calls to it.
pub(crate) struct StateMachine {
    height: BlockNumber,
    id: ValidatorId,
    round: Round,
    step: Step,
    quorum: VotesThreshold,
    round_skip_threshold: VotesThreshold,
    total_weight: u64,
    is_observer: bool,
    // {round: (proposal_id, valid_round)}
    proposals: HashMap<Round, (Option<ProposalCommitment>, Option<Round>)>,
    // {(round, voter): (vote, weight)}
    prevotes: VotesMap,
    precommits: VotesMap,
    // When true, the state machine will wait for a FinishedBuilding event, buffering all other
    // input events in `events_queue`.
    awaiting_finished_building: bool,
    events_queue: VecDeque<StateMachineEvent>,
    locked_value_round: Option<(ProposalCommitment, Round)>,
    valid_value_round: Option<(ProposalCommitment, Round)>,
    prevote_quorum: HashSet<Round>,
    mixed_prevote_quorum: HashSet<Round>,
    mixed_precommit_quorum: HashSet<Round>,
    // Tracks the latest self votes for efficient rebroadcasts.
    last_self_prevote: Option<Vote>,
    last_self_precommit: Option<Vote>,
}

impl StateMachine {
    /// total_weight - the total voting weight of all validators for this height.
    pub(crate) fn new(
        height: BlockNumber,
        id: ValidatorId,
        total_weight: u64,
        is_observer: bool,
        quorum_type: QuorumType,
    ) -> Self {
        Self {
            height,
            id,
            round: 0,
            step: Step::Propose,
            // Byzantine: 2/3 votes, Honest: 1/2 votes.
            quorum: VotesThreshold::from_quorum_type(quorum_type),
            // Skip round threshold is 1/3 of the total weight.
            round_skip_threshold: ROUND_SKIP_THRESHOLD,
            total_weight,
            is_observer,
            proposals: HashMap::new(),
            prevotes: HashMap::new(),
            precommits: HashMap::new(),
            awaiting_finished_building: false,
            events_queue: VecDeque::new(),
            locked_value_round: None,
            valid_value_round: None,
            prevote_quorum: HashSet::new(),
            mixed_prevote_quorum: HashSet::new(),
            mixed_precommit_quorum: HashSet::new(),
            last_self_prevote: None,
            last_self_precommit: None,
        }
    }

    pub(crate) fn round(&self) -> Round {
        self.round
    }

    pub(crate) fn total_weight(&self) -> u64 {
        self.total_weight
    }

    pub(crate) fn quorum(&self) -> &VotesThreshold {
        &self.quorum
    }

    pub(crate) fn height(&self) -> BlockNumber {
        self.height
    }

    pub(crate) fn prevotes_ref(&self) -> &VotesMap {
        &self.prevotes
    }

    pub(crate) fn precommits_ref(&self) -> &VotesMap {
        &self.precommits
    }

    pub(crate) fn has_proposal_for_round(&self, round: Round) -> bool {
        self.proposals.contains_key(&round)
    }

    pub(crate) fn proposal_id_for_round(&self, round: Round) -> Option<ProposalCommitment> {
        self.proposals.get(&round).and_then(|(id, _)| *id)
    }

    pub(crate) fn last_self_prevote(&self) -> Option<Vote> {
        self.last_self_prevote.clone()
    }

    pub(crate) fn last_self_precommit(&self) -> Option<Vote> {
        self.last_self_precommit.clone()
    }

    fn make_self_vote(
        &mut self,
        vote_type: VoteType,
        proposal_commitment: Option<ProposalCommitment>,
    ) -> VecDeque<SMRequest> {
        let vote = Vote {
            vote_type,
            height: self.height.0,
            round: self.round,
            proposal_commitment,
            voter: self.id,
        };
        let mut output = VecDeque::new();
        // Only non-observers record and track self-votes.
        if self.is_observer {
            return output;
        }
        let (votes_map, last_self_vote) = match vote_type {
            VoteType::Prevote => (&mut self.prevotes, &mut self.last_self_prevote),
            VoteType::Precommit => (&mut self.precommits, &mut self.last_self_precommit),
        };
        // Record the vote in the appropriate map.
        let inserted = votes_map.insert((self.round, self.id), (vote.clone(), 1)).is_none();
        assert!(
            inserted,
            "This should never happen: duplicate self {:?} vote for round={}, id={}",
            vote_type, self.round, self.id
        );
        // Update the latest self vote.
        assert!(
            last_self_vote.as_ref().is_none_or(|last| self.round > last.round),
            "State machine must progress in time: last_vote: {last_self_vote:?} new_vote: {vote:?}"
        );
        *last_self_vote = Some(vote.clone());
        // Returns VecDeque instead of a single SMRequest so callers can chain requests using
        // append().
        output.push_back(SMRequest::BroadcastVote(vote));
        output
    }

    /// Starts the state machine, effectively calling `StartRound(0)` from the paper.
    pub(crate) fn start<LeaderFn>(&mut self, leader_fn: &LeaderFn) -> VecDeque<SMRequest>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        self.advance_to_round(0, leader_fn)
    }

    /// Process the incoming event.
    ///
    /// If we are waiting for a response to
    /// [`FinishedBuilding`](`StateMachineEvent::FinishedBuilding`) all other incoming events
    /// are buffered until that response arrives.
    ///
    /// Returns a set of requests for the caller to handle. The caller should handle them and pass
    /// the relevant response back to the state machine.
    pub(crate) fn handle_event<LeaderFn>(
        &mut self,
        event: StateMachineEvent,
        leader_fn: &LeaderFn,
    ) -> VecDeque<SMRequest>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        // Mimic LOC 18 in the paper; the state machine doesn't
        // handle any events until `getValue` completes.
        if self.awaiting_finished_building {
            match event {
                StateMachineEvent::FinishedBuilding(_, round) if round == self.round => {
                    self.events_queue.push_front(event);
                }
                _ => {
                    self.events_queue.push_back(event);
                    return VecDeque::new();
                }
            }
        } else {
            self.events_queue.push_back(event);
        }

        self.handle_enqueued_events(leader_fn)
    }

    pub(crate) fn handle_enqueued_events<LeaderFn>(
        &mut self,
        leader_fn: &LeaderFn,
    ) -> VecDeque<SMRequest>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        let mut output_requests = VecDeque::new();
        while let Some(event) = self.events_queue.pop_front() {
            let mut resultant_requests = self.handle_event_internal(event, leader_fn);
            while let Some(r) = resultant_requests.pop_front() {
                match r {
                    SMRequest::StartBuildProposal(_) => {
                        // LOC 18 in the paper.
                        assert!(resultant_requests.is_empty());
                        assert!(!self.is_observer);
                        output_requests.push_back(r);
                        return output_requests;
                    }
                    SMRequest::DecisionReached(_, _) => {
                        // These requests stop processing and return immediately.
                        output_requests.push_back(r);
                        return output_requests;
                    }
                    SMRequest::BroadcastVote(_) => {
                        assert!(!self.is_observer, "Observers should not broadcast votes");
                        output_requests.push_back(r);
                    }
                    _ => {
                        output_requests.push_back(r);
                    }
                }
            }
        }
        output_requests
    }

    pub(crate) fn handle_event_internal<LeaderFn>(
        &mut self,
        event: StateMachineEvent,
        leader_fn: &LeaderFn,
    ) -> VecDeque<SMRequest>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        trace!("Processing event: {:?}", event);
        if self.awaiting_finished_building {
            assert!(matches!(event, StateMachineEvent::FinishedBuilding(_, _)), "{event:?}");
        }

        match event {
            StateMachineEvent::FinishedBuilding(proposal_id, round) => {
                self.handle_finished_building(proposal_id, round, leader_fn)
            }
            StateMachineEvent::FinishedValidation(proposal_id, round, valid_round) => {
                self.handle_finished_validation(proposal_id, round, valid_round, leader_fn)
            }
            StateMachineEvent::Prevote(vote) => self.handle_prevote(vote, leader_fn),
            StateMachineEvent::Precommit(vote) => self.handle_precommit(vote, leader_fn),
            StateMachineEvent::TimeoutPropose(round) => self.handle_timeout_propose(round),
            StateMachineEvent::TimeoutPrevote(round) => self.handle_timeout_prevote(round),
            StateMachineEvent::TimeoutPrecommit(round) => {
                self.handle_timeout_precommit(round, leader_fn)
            }
            StateMachineEvent::VoteBroadcasted(_) => {
                unreachable!("StateMachine should not receive VoteBroadcasted events");
            }
        }
    }

    pub(crate) fn handle_finished_building<LeaderFn>(
        &mut self,
        proposal_id: Option<ProposalCommitment>,
        round: u32,
        leader_fn: &LeaderFn,
    ) -> VecDeque<SMRequest>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        assert!(self.awaiting_finished_building);
        assert_eq!(round, self.round);
        self.awaiting_finished_building = false;
        let old = self.proposals.insert(round, (proposal_id, None));
        assert!(old.is_none(), "Proposal built when one already exists for this round.");

        self.map_round_to_upons(round, leader_fn)
    }

    pub(crate) fn handle_finished_validation<LeaderFn>(
        &mut self,
        proposal_id: Option<ProposalCommitment>,
        round: u32,
        valid_round: Option<Round>,
        leader_fn: &LeaderFn,
    ) -> VecDeque<SMRequest>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        let old = self.proposals.insert(round, (proposal_id, valid_round));
        assert!(old.is_none(), "SHC should handle conflicts & replays");
        self.map_round_to_upons(round, leader_fn)
    }

    pub(crate) fn handle_timeout_propose(&mut self, round: u32) -> VecDeque<SMRequest> {
        if self.step != Step::Propose || round != self.round {
            return VecDeque::new();
        };
        warn!(
            "PROPOSAL_FAILED: Proposal failed as validator. Applying TimeoutPropose for \
             round={round}."
        );
        CONSENSUS_TIMEOUTS.increment(1, &[(LABEL_NAME_TIMEOUT_TYPE, TimeoutType::Propose.into())]);
        let mut output = self.make_self_vote(VoteType::Prevote, None);
        output.append(&mut self.advance_to_step(Step::Prevote));
        output
    }

    // A prevote from a peer node.
    pub(crate) fn handle_prevote<LeaderFn>(
        &mut self,
        vote: Vote,
        leader_fn: &LeaderFn,
    ) -> VecDeque<SMRequest>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        let round = vote.round;
        let voter = vote.voter;
        let inserted = self.prevotes.insert((round, voter), (vote, 1)).is_none();
        assert!(
            inserted,
            "SHC should handle conflicts & replays: duplicate prevote for round={round}, \
             voter={voter}",
        );
        self.map_round_to_upons(round, leader_fn)
    }

    pub(crate) fn handle_timeout_prevote(&mut self, round: u32) -> VecDeque<SMRequest> {
        if self.step != Step::Prevote || round != self.round {
            return VecDeque::new();
        };
        debug!("Applying TimeoutPrevote for round={round}.");
        CONSENSUS_TIMEOUTS.increment(1, &[(LABEL_NAME_TIMEOUT_TYPE, TimeoutType::Prevote.into())]);
        let mut output = self.make_self_vote(VoteType::Precommit, None);
        output.append(&mut self.advance_to_step(Step::Precommit));
        output
    }

    // A precommit from a peer node.
    fn handle_precommit<LeaderFn>(
        &mut self,
        vote: Vote,
        leader_fn: &LeaderFn,
    ) -> VecDeque<SMRequest>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        let round = vote.round;
        let voter = vote.voter;
        let inserted = self.precommits.insert((round, voter), (vote, 1)).is_none();
        assert!(
            inserted,
            "SHC should handle conflicts & replays: duplicate precommit for round={round}, \
             voter={voter}"
        );
        self.map_round_to_upons(round, leader_fn)
    }

    fn handle_timeout_precommit<LeaderFn>(
        &mut self,
        round: u32,
        leader_fn: &LeaderFn,
    ) -> VecDeque<SMRequest>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        if round != self.round {
            return VecDeque::new();
        };
        debug!("Applying TimeoutPrecommit for round={round}.");
        CONSENSUS_TIMEOUTS
            .increment(1, &[(LABEL_NAME_TIMEOUT_TYPE, TimeoutType::Precommit.into())]);
        self.advance_to_round(round + 1, leader_fn)
    }

    // LOC 11 in the paper.
    fn advance_to_round<LeaderFn>(
        &mut self,
        round: u32,
        leader_fn: &LeaderFn,
    ) -> VecDeque<SMRequest>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        CONSENSUS_ROUND.set(round);
        if round > 0 {
            CONSENSUS_ROUND_ADVANCES.increment(1);
            // Count how many times consensus advanced above round 0.
            if round == 1 {
                CONSENSUS_ROUND_ABOVE_ZERO.increment(1);
            }
        }
        if self.locked_value_round.is_some() {
            CONSENSUS_HELD_LOCKS.increment(1);
        }
        self.round = round;
        self.step = Step::Propose;
        let mut output = if !self.is_observer && self.id == leader_fn(self.round) {
            info!("START_ROUND_PROPOSER: Starting round {round} as Proposer");
            // Leader.
            match self.valid_value_round {
                Some((proposal_id, valid_round)) => {
                    // Record the valid proposal for the current round so upon_reproposal() can
                    // observe it and emit the corresponding prevote immediately.
                    let old =
                        self.proposals.insert(self.round, (Some(proposal_id), Some(valid_round)));
                    assert!(old.is_none(), "Proposal for current round should not already exist");
                    let init = ProposalInit {
                        height: self.height,
                        round: self.round,
                        proposer: self.id,
                        valid_round: Some(valid_round),
                    };
                    VecDeque::from([SMRequest::Repropose(proposal_id, init)])
                }
                None => {
                    self.awaiting_finished_building = true;
                    // Upon conditions are not checked while awaiting a new proposal.
                    return VecDeque::from([SMRequest::StartBuildProposal(self.round)]);
                }
            }
        } else {
            info!("START_ROUND_VALIDATOR: Starting round {round} as Validator");
            VecDeque::from([SMRequest::ScheduleTimeoutPropose(self.round)])
        };
        output.append(&mut self.current_round_upons());
        output
    }

    fn advance_to_step(&mut self, step: Step) -> VecDeque<SMRequest> {
        assert_ne!(step, Step::Propose, "Advancing to Propose is done by advancing rounds");
        info!("Advancing step: from {:?} to {step:?} in round={}", self.step, self.round);
        self.step = step;
        self.current_round_upons()
    }

    fn map_round_to_upons<LeaderFn>(
        &mut self,
        round: u32,
        leader_fn: &LeaderFn,
    ) -> VecDeque<SMRequest>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        match round.cmp(&self.round) {
            std::cmp::Ordering::Less => self.past_round_upons(round),
            std::cmp::Ordering::Equal => self.current_round_upons(),
            std::cmp::Ordering::Greater => self.maybe_advance_to_round(round, leader_fn),
        }
    }

    fn current_round_upons(&mut self) -> VecDeque<SMRequest> {
        let mut output = VecDeque::new();
        output.append(&mut self.upon_new_proposal());
        output.append(&mut self.upon_reproposal());
        output.append(&mut self.maybe_initiate_timeout_prevote());
        output.append(&mut self.upon_prevote_quorum());
        output.append(&mut self.upon_nil_prevote_quorum());
        output.append(&mut self.maybe_initiate_timeout_precommit());
        output.append(&mut self.upon_decision(self.round));
        output
    }

    fn past_round_upons(&mut self, round: u32) -> VecDeque<SMRequest> {
        let mut output = VecDeque::new();
        output.append(&mut self.upon_reproposal());
        output.append(&mut self.upon_decision(round));
        output
    }

    // LOC 22 in the paper.
    fn upon_new_proposal(&mut self) -> VecDeque<SMRequest> {
        // StateMachine assumes that the proposer is valid.
        if self.step != Step::Propose {
            return VecDeque::new();
        }
        let Some((proposal_id, valid_round)) = self.proposals.get(&self.round) else {
            return VecDeque::new();
        };
        if valid_round.is_some() {
            return VecDeque::new();
        }
        let vote_commitment = if proposal_id.is_some_and(|v| {
            self.locked_value_round.is_none_or(|(locked_value, _)| v == locked_value)
        }) {
            *proposal_id
        } else {
            None
        };
        let mut output = self.make_self_vote(VoteType::Prevote, vote_commitment);
        output.append(&mut self.advance_to_step(Step::Prevote));
        output
    }

    // LOC 28 in the paper.
    fn upon_reproposal(&mut self) -> VecDeque<SMRequest> {
        if self.step != Step::Propose {
            return VecDeque::new();
        }
        let Some((proposal_id, valid_round)) = self.proposals.get(&self.round) else {
            return VecDeque::new();
        };
        let Some(valid_round) = valid_round else {
            return VecDeque::new();
        };
        if valid_round >= &self.round {
            return VecDeque::new();
        }
        if !self.value_has_enough_votes(&self.prevotes, *valid_round, proposal_id, &self.quorum) {
            return VecDeque::new();
        }
        let vote_commitment = if proposal_id.is_some_and(|v| {
            self.locked_value_round.is_none_or(|(locked_value, locked_round)| {
                locked_round <= *valid_round || locked_value == v
            })
        }) {
            *proposal_id
        } else {
            None
        };
        let mut output = self.make_self_vote(VoteType::Prevote, vote_commitment);
        output.append(&mut self.advance_to_step(Step::Prevote));
        output
    }

    // LOC 34 in the paper.
    fn maybe_initiate_timeout_prevote(&mut self) -> VecDeque<SMRequest> {
        if self.step != Step::Prevote {
            return VecDeque::new();
        }
        if !self.round_has_enough_votes(&self.prevotes, self.round, &self.quorum) {
            return VecDeque::new();
        }
        // Getting mixed prevote quorum for the first time.
        if !self.mixed_prevote_quorum.insert(self.round) {
            return VecDeque::new();
        }
        VecDeque::from([SMRequest::ScheduleTimeoutPrevote(self.round)])
    }

    // LOC 36 in the paper.
    fn upon_prevote_quorum(&mut self) -> VecDeque<SMRequest> {
        if self.step == Step::Propose {
            return VecDeque::new();
        }
        let Some((Some(proposal_id), _)) = self.proposals.get(&self.round) else {
            return VecDeque::new();
        };
        if !self.value_has_enough_votes(
            &self.prevotes,
            self.round,
            &Some(*proposal_id),
            &self.quorum,
        ) {
            return VecDeque::new();
        }
        // Getting prevote quorum for the first time.
        if !self.prevote_quorum.insert(self.round) {
            return VecDeque::new();
        }
        self.valid_value_round = Some((*proposal_id, self.round));
        if self.step != Step::Prevote {
            return VecDeque::new();
        }
        let new_value = Some((*proposal_id, self.round));
        if new_value != self.locked_value_round {
            CONSENSUS_NEW_VALUE_LOCKS.increment(1);
        }
        self.locked_value_round = new_value;
        let mut output = self.make_self_vote(VoteType::Precommit, Some(*proposal_id));
        output.append(&mut self.advance_to_step(Step::Precommit));
        output
    }

    // LOC 44 in the paper.
    fn upon_nil_prevote_quorum(&mut self) -> VecDeque<SMRequest> {
        if self.step != Step::Prevote {
            return VecDeque::new();
        }
        if !self.value_has_enough_votes(&self.prevotes, self.round, &None, &self.quorum) {
            return VecDeque::new();
        }
        let mut output = self.make_self_vote(VoteType::Precommit, None);
        output.append(&mut self.advance_to_step(Step::Precommit));
        output
    }

    // LOC 47 in the paper.
    fn maybe_initiate_timeout_precommit(&mut self) -> VecDeque<SMRequest> {
        if !self.round_has_enough_votes(&self.precommits, self.round, &self.quorum) {
            return VecDeque::new();
        }
        // Getting mixed precommit quorum for the first time.
        if !self.mixed_precommit_quorum.insert(self.round) {
            return VecDeque::new();
        }
        VecDeque::from([SMRequest::ScheduleTimeoutPrecommit(self.round)])
    }

    // LOC 49 in the paper.
    fn upon_decision(&mut self, round: u32) -> VecDeque<SMRequest> {
        let Some((Some(proposal_id), _)) = self.proposals.get(&round) else {
            return VecDeque::new();
        };
        if !self.value_has_enough_votes(&self.precommits, round, &Some(*proposal_id), &self.quorum)
        {
            return VecDeque::new();
        }

        VecDeque::from([SMRequest::DecisionReached(*proposal_id, round)])
    }

    // LOC 55 in the paper.
    fn maybe_advance_to_round<LeaderFn>(
        &mut self,
        round: u32,
        leader_fn: &LeaderFn,
    ) -> VecDeque<SMRequest>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        if self.round_has_enough_votes(&self.prevotes, round, &self.round_skip_threshold)
            || self.round_has_enough_votes(&self.precommits, round, &self.round_skip_threshold)
        {
            self.advance_to_round(round, leader_fn)
        } else {
            VecDeque::new()
        }
    }

    fn round_has_enough_votes(
        &self,
        votes: &VotesMap,
        round: u32,
        threshold: &VotesThreshold,
    ) -> bool {
        // TODO(Asmaa): Refactor round_has_enough_votes and value_has_enough_votes to use a shared
        // weighted sum calculator.
        let weight_sum = votes
            .iter()
            .filter_map(
                |(&(r, _voter), (_v, w))| if r == round { Some(u64::from(*w)) } else { None },
            )
            .sum();
        threshold.is_met(weight_sum, self.total_weight)
    }

    fn value_has_enough_votes(
        &self,
        votes: &VotesMap,
        round: u32,
        value: &Option<ProposalCommitment>,
        threshold: &VotesThreshold,
    ) -> bool {
        let weight_sum = votes
            .iter()
            .filter_map(|(&(r, _voter), (v, w))| {
                if r == round && &v.proposal_commitment == value {
                    Some(u64::from(*w))
                } else {
                    None
                }
            })
            .sum();
        threshold.is_met(weight_sum, self.total_weight)
    }
}
