//! State machine for Starknet consensus.
//!
//! LOC refers to the line of code from Algorithm 1 (page 6) of the tendermint
//! [paper](https://arxiv.org/pdf/1807.04938).

#[cfg(test)]
#[path = "state_machine_test.rs"]
mod state_machine_test;

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};
use tracing::{debug, info, trace, warn};

use crate::metrics::{
    TimeoutReason,
    CONSENSUS_HELD_LOCKS,
    CONSENSUS_NEW_VALUE_LOCKS,
    CONSENSUS_ROUND,
    CONSENSUS_ROUND_ABOVE_ZERO,
    CONSENSUS_TIMEOUTS,
    LABEL_NAME_TIMEOUT_REASON,
};
use crate::types::{ProposalCommitment, Round, ValidatorId};
use crate::votes_threshold::{QuorumType, VotesThreshold, ROUND_SKIP_THRESHOLD};

/// Events which the state machine sends/receives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StateMachineEvent {
    /// Sent by the state machine when a block is required to propose (ProposalCommitment is always
    /// None). While waiting for the response of GetProposal, the state machine will buffer all
    /// other events. The caller *must* respond with a valid proposal id for this height to the
    /// state machine, and the same round sent out.
    GetProposal(Option<ProposalCommitment>, Round),
    /// Consensus message, can be both sent from and to the state machine.
    // (proposal_id, round, valid_round)
    Proposal(Option<ProposalCommitment>, Round, Option<Round>),
    /// Consensus message, can be both sent from and to the state machine.
    Prevote(Option<ProposalCommitment>, Round),
    /// Consensus message, can be both sent from and to the state machine.
    Precommit(Option<ProposalCommitment>, Round),
    /// The state machine returns this event to the caller when a decision is reached. Not
    /// expected as an inbound message. We presume that the caller is able to recover the set of
    /// precommits which led to this decision from the information returned here.
    Decision(ProposalCommitment, Round),
    /// Timeout events, can be both sent from and to the state machine.
    TimeoutPropose(Round),
    /// Timeout events, can be both sent from and to the state machine.
    TimeoutPrevote(Round),
    /// Timeout events, can be both sent from and to the state machine.
    TimeoutPrecommit(Round),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Step {
    Propose,
    Prevote,
    Precommit,
}

/// State Machine. Major assumptions:
/// 1. SHC handles: authentication, replays, and conflicts.
/// 2. SM must handle "out of order" messages (E.g. vote arrives before proposal).
///
/// Each height is begun with a call to `start`, with no further calls to it.
#[derive(Serialize, Deserialize)]
pub struct StateMachine {
    id: ValidatorId,
    round: Round,
    step: Step,
    quorum: VotesThreshold,
    round_skip_threshold: VotesThreshold,
    total_weight: u64,
    is_observer: bool,
    // {round: (proposal_id, valid_round)}
    proposals: HashMap<Round, (Option<ProposalCommitment>, Option<Round>)>,
    // {round: {proposal_id: vote_count}
    prevotes: HashMap<Round, HashMap<Option<ProposalCommitment>, u32>>,
    precommits: HashMap<Round, HashMap<Option<ProposalCommitment>, u32>>,
    // When true, the state machine will wait for a GetProposal event, buffering all other input
    // events in `events_queue`.
    awaiting_get_proposal: bool,
    events_queue: VecDeque<StateMachineEvent>,
    locked_value_round: Option<(ProposalCommitment, Round)>,
    valid_value_round: Option<(ProposalCommitment, Round)>,
    prevote_quorum: HashSet<Round>,
    mixed_prevote_quorum: HashSet<Round>,
    mixed_precommit_quorum: HashSet<Round>,
}

impl StateMachine {
    /// total_weight - the total voting weight of all validators for this height.
    pub fn new(
        id: ValidatorId,
        total_weight: u64,
        is_observer: bool,
        quorum_type: QuorumType,
    ) -> Self {
        Self {
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
            awaiting_get_proposal: false,
            events_queue: VecDeque::new(),
            locked_value_round: None,
            valid_value_round: None,
            prevote_quorum: HashSet::new(),
            mixed_prevote_quorum: HashSet::new(),
            mixed_precommit_quorum: HashSet::new(),
        }
    }

    pub fn round(&self) -> Round {
        self.round
    }

    pub fn total_weight(&self) -> u64 {
        self.total_weight
    }

    pub fn quorum(&self) -> &VotesThreshold {
        &self.quorum
    }

    /// Starts the state machine, effectively calling `StartRound(0)` from the paper. This is
    /// needed to trigger the first leader to propose.
    /// See [`GetProposal`](StateMachineEvent::GetProposal)
    pub fn start<LeaderFn>(&mut self, leader_fn: &LeaderFn) -> VecDeque<StateMachineEvent>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        self.advance_to_round(0, leader_fn)
    }

    /// Process the incoming event.
    ///
    /// If we are waiting for a response to [`GetProposal`](`StateMachineEvent::GetProposal`) all
    /// other incoming events are buffered until that response arrives.
    ///
    /// Returns a set of events for the caller to handle. The caller should not mirror the output
    /// events back to the state machine, as it makes sure to handle them before returning.
    // This means that the StateMachine handles events the same regardless of whether it was sent by
    // self or a peer. This is in line with the Algorithm 1 in the paper and keeps the code simpler.
    pub fn handle_event<LeaderFn>(
        &mut self,
        event: StateMachineEvent,
        leader_fn: &LeaderFn,
    ) -> VecDeque<StateMachineEvent>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        // Mimic LOC 18 in the paper; the state machine doesn't
        // handle any events until `getValue` completes.
        if self.awaiting_get_proposal {
            match event {
                StateMachineEvent::GetProposal(_, round) if round == self.round => {
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

    fn handle_enqueued_events<LeaderFn>(
        &mut self,
        leader_fn: &LeaderFn,
    ) -> VecDeque<StateMachineEvent>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        let mut output_events = VecDeque::new();
        while let Some(event) = self.events_queue.pop_front() {
            // Handle a specific event and then decide which of the output events should also be
            // sent to self.
            let mut resultant_events = self.handle_event_internal(event, leader_fn);
            while let Some(e) = resultant_events.pop_front() {
                match e {
                    StateMachineEvent::Proposal(_, _, _)
                    | StateMachineEvent::Prevote(_, _)
                    | StateMachineEvent::Precommit(_, _) => {
                        if self.is_observer {
                            continue;
                        }
                        self.events_queue.push_back(e.clone());
                    }
                    StateMachineEvent::Decision(_, _) => {
                        output_events.push_back(e);
                        return output_events;
                    }
                    StateMachineEvent::GetProposal(_, _) => {
                        // LOC 18.
                        assert!(resultant_events.is_empty());
                        assert!(!self.is_observer);
                        output_events.push_back(e);
                        return output_events;
                    }
                    _ => {}
                }
                output_events.push_back(e);
            }
        }
        output_events
    }

    fn handle_event_internal<LeaderFn>(
        &mut self,
        event: StateMachineEvent,
        leader_fn: &LeaderFn,
    ) -> VecDeque<StateMachineEvent>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        trace!("Processing event: {:?}", event);
        if self.awaiting_get_proposal {
            assert!(matches!(event, StateMachineEvent::GetProposal(_, _)), "{event:?}");
        }

        match event {
            StateMachineEvent::GetProposal(proposal_id, round) => {
                self.handle_get_proposal(proposal_id, round)
            }
            StateMachineEvent::Proposal(proposal_id, round, valid_round) => {
                self.handle_proposal(proposal_id, round, valid_round, leader_fn)
            }
            StateMachineEvent::Prevote(proposal_id, round) => {
                self.handle_prevote(proposal_id, round, leader_fn)
            }
            StateMachineEvent::Precommit(proposal_id, round) => {
                self.handle_precommit(proposal_id, round, leader_fn)
            }
            StateMachineEvent::Decision(_, _) => {
                unimplemented!(
                    "If the caller knows of a decision, it can just drop the state machine."
                )
            }
            StateMachineEvent::TimeoutPropose(round) => self.handle_timeout_propose(round),
            StateMachineEvent::TimeoutPrevote(round) => self.handle_timeout_prevote(round),
            StateMachineEvent::TimeoutPrecommit(round) => {
                self.handle_timeout_precommit(round, leader_fn)
            }
        }
    }

    fn handle_get_proposal(
        &mut self,
        proposal_id: Option<ProposalCommitment>,
        round: u32,
    ) -> VecDeque<StateMachineEvent> {
        // TODO(matan): Will we allow other events (timeoutPropose) to exit this state?
        assert!(self.awaiting_get_proposal);
        assert_eq!(round, self.round);
        self.awaiting_get_proposal = false;
        VecDeque::from([StateMachineEvent::Proposal(proposal_id, round, None)])
    }

    // A proposal from a peer (or self) node.
    fn handle_proposal<LeaderFn>(
        &mut self,
        proposal_id: Option<ProposalCommitment>,
        round: u32,
        valid_round: Option<Round>,
        leader_fn: &LeaderFn,
    ) -> VecDeque<StateMachineEvent>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        let old = self.proposals.insert(round, (proposal_id, valid_round));
        assert!(old.is_none(), "SHC should handle conflicts & replays");
        self.map_round_to_upons(round, leader_fn)
    }

    fn handle_timeout_propose(&mut self, round: u32) -> VecDeque<StateMachineEvent> {
        if self.step != Step::Propose || round != self.round {
            return VecDeque::new();
        };
        warn!("Proposal failed. Applying TimeoutPropose for round={round}.");
        CONSENSUS_TIMEOUTS
            .increment(1, &[(LABEL_NAME_TIMEOUT_REASON, TimeoutReason::Propose.into())]);
        let mut output = VecDeque::from([StateMachineEvent::Prevote(None, round)]);
        output.append(&mut self.advance_to_step(Step::Prevote));
        output
    }

    // A prevote from a peer (or self) node.
    fn handle_prevote<LeaderFn>(
        &mut self,
        proposal_id: Option<ProposalCommitment>,
        round: u32,
        leader_fn: &LeaderFn,
    ) -> VecDeque<StateMachineEvent>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        let prevote_count = self.prevotes.entry(round).or_default().entry(proposal_id).or_insert(0);
        // TODO(matan): Use variable weight.
        *prevote_count += 1;
        self.map_round_to_upons(round, leader_fn)
    }

    fn handle_timeout_prevote(&mut self, round: u32) -> VecDeque<StateMachineEvent> {
        if self.step != Step::Prevote || round != self.round {
            return VecDeque::new();
        };
        debug!("Applying TimeoutPrevote for round={round}.");
        CONSENSUS_TIMEOUTS
            .increment(1, &[(LABEL_NAME_TIMEOUT_REASON, TimeoutReason::Prevote.into())]);
        let mut output = VecDeque::from([StateMachineEvent::Precommit(None, round)]);
        output.append(&mut self.advance_to_step(Step::Precommit));
        output
    }

    // A precommit from a peer (or self) node.
    fn handle_precommit<LeaderFn>(
        &mut self,
        proposal_id: Option<ProposalCommitment>,
        round: u32,
        leader_fn: &LeaderFn,
    ) -> VecDeque<StateMachineEvent>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        let precommit_count =
            self.precommits.entry(round).or_default().entry(proposal_id).or_insert(0);
        // TODO(matan): Use variable weight.
        *precommit_count += 1;
        self.map_round_to_upons(round, leader_fn)
    }

    fn handle_timeout_precommit<LeaderFn>(
        &mut self,
        round: u32,
        leader_fn: &LeaderFn,
    ) -> VecDeque<StateMachineEvent>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        if round != self.round {
            return VecDeque::new();
        };
        debug!("Applying TimeoutPrecommit for round={round}.");
        CONSENSUS_TIMEOUTS
            .increment(1, &[(LABEL_NAME_TIMEOUT_REASON, TimeoutReason::Precommit.into())]);
        self.advance_to_round(round + 1, leader_fn)
    }

    // LOC 11 in the paper.
    fn advance_to_round<LeaderFn>(
        &mut self,
        round: u32,
        leader_fn: &LeaderFn,
    ) -> VecDeque<StateMachineEvent>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        CONSENSUS_ROUND.set(round);
        // Count how many times consensus advanced above round 0.
        if round == 1 {
            CONSENSUS_ROUND_ABOVE_ZERO.increment(1);
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
                Some((proposal_id, valid_round)) => VecDeque::from([StateMachineEvent::Proposal(
                    Some(proposal_id),
                    self.round,
                    Some(valid_round),
                )]),
                None => {
                    self.awaiting_get_proposal = true;
                    // Upon conditions are not checked while awaiting a new proposal.
                    return VecDeque::from([StateMachineEvent::GetProposal(None, self.round)]);
                }
            }
        } else {
            info!("START_ROUND_VALIDATOR: Starting round {round} as Validator");
            VecDeque::from([StateMachineEvent::TimeoutPropose(self.round)])
        };
        output.append(&mut self.current_round_upons());
        output
    }

    fn advance_to_step(&mut self, step: Step) -> VecDeque<StateMachineEvent> {
        assert_ne!(step, Step::Propose, "Advancing to Propose is done by advancing rounds");
        info!("Advancing step: from {:?} to {step:?} in round={}", self.step, self.round);
        self.step = step;
        self.current_round_upons()
    }

    fn map_round_to_upons<LeaderFn>(
        &mut self,
        round: u32,
        leader_fn: &LeaderFn,
    ) -> VecDeque<StateMachineEvent>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        match round.cmp(&self.round) {
            std::cmp::Ordering::Less => self.past_round_upons(round),
            std::cmp::Ordering::Equal => self.current_round_upons(),
            std::cmp::Ordering::Greater => self.maybe_advance_to_round(round, leader_fn),
        }
    }

    fn current_round_upons(&mut self) -> VecDeque<StateMachineEvent> {
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

    fn past_round_upons(&mut self, round: u32) -> VecDeque<StateMachineEvent> {
        let mut output = VecDeque::new();
        output.append(&mut self.upon_reproposal());
        output.append(&mut self.upon_decision(round));
        output
    }

    // LOC 22 in the paper.
    fn upon_new_proposal(&mut self) -> VecDeque<StateMachineEvent> {
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
        let mut output = if proposal_id.is_some_and(|v| {
            self.locked_value_round.is_none_or(|(locked_value, _)| v == locked_value)
        }) {
            VecDeque::from([StateMachineEvent::Prevote(*proposal_id, self.round)])
        } else {
            VecDeque::from([StateMachineEvent::Prevote(None, self.round)])
        };
        output.append(&mut self.advance_to_step(Step::Prevote));
        output
    }

    // LOC 28 in the paper.
    fn upon_reproposal(&mut self) -> VecDeque<StateMachineEvent> {
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
        let mut output = if proposal_id.is_some_and(|v| {
            self.locked_value_round.is_none_or(|(locked_value, locked_round)| {
                locked_round <= *valid_round || locked_value == v
            })
        }) {
            VecDeque::from([StateMachineEvent::Prevote(*proposal_id, self.round)])
        } else {
            VecDeque::from([StateMachineEvent::Prevote(None, self.round)])
        };
        output.append(&mut self.advance_to_step(Step::Prevote));
        output
    }

    // LOC 34 in the paper.
    fn maybe_initiate_timeout_prevote(&mut self) -> VecDeque<StateMachineEvent> {
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
        VecDeque::from([StateMachineEvent::TimeoutPrevote(self.round)])
    }

    // LOC 36 in the paper.
    fn upon_prevote_quorum(&mut self) -> VecDeque<StateMachineEvent> {
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
        let mut output =
            VecDeque::from([StateMachineEvent::Precommit(Some(*proposal_id), self.round)]);
        output.append(&mut self.advance_to_step(Step::Precommit));
        output
    }

    // LOC 44 in the paper.
    fn upon_nil_prevote_quorum(&mut self) -> VecDeque<StateMachineEvent> {
        if self.step != Step::Prevote {
            return VecDeque::new();
        }
        if !self.value_has_enough_votes(&self.prevotes, self.round, &None, &self.quorum) {
            return VecDeque::new();
        }
        let mut output = VecDeque::from([StateMachineEvent::Precommit(None, self.round)]);
        output.append(&mut self.advance_to_step(Step::Precommit));
        output
    }

    // LOC 47 in the paper.
    fn maybe_initiate_timeout_precommit(&mut self) -> VecDeque<StateMachineEvent> {
        if !self.round_has_enough_votes(&self.precommits, self.round, &self.quorum) {
            return VecDeque::new();
        }
        // Getting mixed precommit quorum for the first time.
        if !self.mixed_precommit_quorum.insert(self.round) {
            return VecDeque::new();
        }
        VecDeque::from([StateMachineEvent::TimeoutPrecommit(self.round)])
    }

    // LOC 49 in the paper.
    fn upon_decision(&mut self, round: u32) -> VecDeque<StateMachineEvent> {
        let Some((Some(proposal_id), _)) = self.proposals.get(&round) else {
            return VecDeque::new();
        };
        if !self.value_has_enough_votes(&self.precommits, round, &Some(*proposal_id), &self.quorum)
        {
            return VecDeque::new();
        }

        VecDeque::from([StateMachineEvent::Decision(*proposal_id, round)])
    }

    // LOC 55 in the paper.
    fn maybe_advance_to_round<LeaderFn>(
        &mut self,
        round: u32,
        leader_fn: &LeaderFn,
    ) -> VecDeque<StateMachineEvent>
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
        votes: &HashMap<u32, HashMap<Option<ProposalCommitment>, u32>>,
        round: u32,
        threshold: &VotesThreshold,
    ) -> bool {
        threshold
            .is_met(votes.get(&round).map_or(0, |v| v.values().sum()).into(), self.total_weight)
    }

    fn value_has_enough_votes(
        &self,
        votes: &HashMap<u32, HashMap<Option<ProposalCommitment>, u32>>,
        round: u32,
        value: &Option<ProposalCommitment>,
        threshold: &VotesThreshold,
    ) -> bool {
        threshold.is_met(
            votes.get(&round).map_or(0, |v| *v.get(value).unwrap_or(&0)).into(),
            self.total_weight,
        )
    }
}
