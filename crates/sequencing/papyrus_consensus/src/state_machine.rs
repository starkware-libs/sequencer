//! State machine for Starknet consensus.
//!
//! LOC refers to the line of code from Algorithm 1 (page 6) of the tendermint
//! [paper](https://arxiv.org/pdf/1807.04938).

#[cfg(test)]
#[path = "state_machine_test.rs"]
mod state_machine_test;

use std::collections::{HashMap, VecDeque};

use starknet_api::block::BlockHash;
use tracing::{error, trace};

use crate::types::{ProposalContentId, Round, ValidatorId};

/// Events which the state machine sends/receives.
#[derive(Debug, Clone, PartialEq)]
pub enum StateMachineEvent {
    /// Sent by the state machine when a block is required to propose (BlockHash is always None).
    /// While waiting for the response of GetProposal, the state machine will buffer all other
    /// events. The caller must respond with a valid block hash for this height to the state
    /// machine, and the same round sent out.
    GetProposal(Option<BlockHash>, Round),
    /// Consensus message, can be both sent from and to the state machine.
    Proposal(Option<BlockHash>, Round, Option<Round>), // (block_hash, round, valid_round)
    /// Consensus message, can be both sent from and to the state machine.
    Prevote(Option<BlockHash>, Round),
    /// Consensus message, can be both sent from and to the state machine.
    Precommit(Option<BlockHash>, Round),
    /// The state machine returns this event to the caller when a decision is reached. Not
    /// expected as an inbound message. We presume that the caller is able to recover the set of
    /// precommits which led to this decision from the information returned here.
    Decision(BlockHash, Round),
    /// Timeout events, can be both sent from and to the state machine.
    TimeoutPropose(Round),
    /// Timeout events, can be both sent from and to the state machine.
    TimeoutPrevote(Round),
    /// Timeout events, can be both sent from and to the state machine.
    TimeoutPrecommit(Round),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Propose,
    Prevote,
    Precommit,
}

/// State Machine. Major assumptions:
/// 1. SHC handles replays and conflicts.
/// 2. SM must handle "out of order" messages (E.g. vote arrives before proposal).
/// 3. No network failures.
pub struct StateMachine {
    id: ValidatorId,
    round: Round,
    step: Step,
    quorum: u32,
    round_skip_threshold: u32,
    // {round: (block_hash, valid_round)}
    proposals: HashMap<Round, (Option<BlockHash>, Option<Round>)>,
    // {round: {block_hash: vote_count}
    prevotes: HashMap<Round, HashMap<Option<BlockHash>, u32>>,
    precommits: HashMap<Round, HashMap<Option<BlockHash>, u32>>,
    // When true, the state machine will wait for a GetProposal event, buffering all other input
    // events in `events_queue`.
    awaiting_get_proposal: bool,
    events_queue: VecDeque<StateMachineEvent>,
    locked_value: Option<(ProposalContentId, Round)>,
}

impl StateMachine {
    /// total_weight - the total voting weight of all validators for this height.
    pub fn new(id: ValidatorId, total_weight: u32) -> Self {
        Self {
            id,
            round: 0,
            step: Step::Propose,
            quorum: (2 * total_weight / 3) + 1,
            round_skip_threshold: total_weight / 3 + 1,
            proposals: HashMap::new(),
            prevotes: HashMap::new(),
            precommits: HashMap::new(),
            awaiting_get_proposal: false,
            events_queue: VecDeque::new(),
            locked_value: None,
        }
    }

    pub fn quorum_size(&self) -> u32 {
        self.quorum
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
    /// If we are waiting for a response to `GetProposal` all other incoming events are buffered
    /// until that response arrives.
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
        trace!("Handling event: {:?}", event);
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
                        self.events_queue.push_back(e.clone());
                    }
                    StateMachineEvent::Decision(_, _) => {
                        output_events.push_back(e);
                        return output_events;
                    }
                    StateMachineEvent::GetProposal(_, _) => {
                        // LOC 18.
                        debug_assert!(resultant_events.is_empty());
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
        if self.awaiting_get_proposal {
            debug_assert!(matches!(event, StateMachineEvent::GetProposal(_, _)), "{:?}", event);
        }

        match event {
            StateMachineEvent::GetProposal(block_hash, round) => {
                self.handle_get_proposal(block_hash, round)
            }
            StateMachineEvent::Proposal(block_hash, round, valid_round) => {
                self.handle_proposal(block_hash, round, valid_round, leader_fn)
            }
            StateMachineEvent::Prevote(block_hash, round) => {
                self.handle_prevote(block_hash, round, leader_fn)
            }
            StateMachineEvent::Precommit(block_hash, round) => {
                self.handle_precommit(block_hash, round, leader_fn)
            }
            StateMachineEvent::Decision(_, _) => {
                unimplemented!(
                    "If the caller knows of a decision, it can just drop the state machine."
                )
            }
            StateMachineEvent::TimeoutPropose(round) => self.handle_timeout_proposal(round),
            StateMachineEvent::TimeoutPrevote(round) => self.handle_timeout_prevote(round),
            StateMachineEvent::TimeoutPrecommit(round) => {
                self.handle_timeout_precommit(round, leader_fn)
            }
        }
    }

    fn handle_get_proposal(
        &mut self,
        block_hash: Option<BlockHash>,
        round: u32,
    ) -> VecDeque<StateMachineEvent> {
        // TODO(matan): Will we allow other events (timeoutPropose) to exit this state?
        assert!(self.awaiting_get_proposal);
        assert_eq!(round, self.round);
        self.awaiting_get_proposal = false;
        assert!(block_hash.is_some(), "SHC should pass a valid block hash");
        VecDeque::from([StateMachineEvent::Proposal(block_hash, round, None)])
    }

    // A proposal from a peer (or self) node.
    fn handle_proposal<LeaderFn>(
        &mut self,
        block_hash: Option<BlockHash>,
        round: u32,
        valid_round: Option<Round>,
        leader_fn: &LeaderFn,
    ) -> VecDeque<StateMachineEvent>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        let old = self.proposals.insert(round, (block_hash, valid_round));
        assert!(old.is_none(), "SHC should handle conflicts & replays");
        match round.cmp(&self.round) {
            std::cmp::Ordering::Less => self.past_round_upons(round),
            std::cmp::Ordering::Greater => self.future_round_upons(round, leader_fn),
            std::cmp::Ordering::Equal => self.process_proposal(block_hash, round, leader_fn),
        }
    }

    fn process_proposal<LeaderFn>(
        &mut self,
        block_hash: Option<BlockHash>,
        round: u32,
        leader_fn: &LeaderFn,
    ) -> VecDeque<StateMachineEvent>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        if self.step != Step::Propose {
            return VecDeque::new();
        }

        let mut output = VecDeque::from([StateMachineEvent::Prevote(block_hash, round)]);
        output.append(&mut self.advance_to_step(Step::Prevote, leader_fn));
        output
    }

    fn handle_timeout_proposal(&mut self, round: u32) -> VecDeque<StateMachineEvent> {
        if self.step != Step::Propose || round != self.round {
            return VecDeque::new();
        };
        self.step = Step::Prevote;
        VecDeque::from([StateMachineEvent::Prevote(None, round)])
    }

    // A prevote from a peer (or self) node.
    fn handle_prevote<LeaderFn>(
        &mut self,
        block_hash: Option<BlockHash>,
        round: u32,
        leader_fn: &LeaderFn,
    ) -> VecDeque<StateMachineEvent>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        let prevote_count = self.prevotes.entry(round).or_default().entry(block_hash).or_insert(0);
        // TODO(matan): Use variable weight.
        *prevote_count += 1;
        match round.cmp(&self.round) {
            std::cmp::Ordering::Less => self.past_round_upons(round),
            std::cmp::Ordering::Greater => self.future_round_upons(round, leader_fn),
            std::cmp::Ordering::Equal => {
                if self.step != Step::Prevote {
                    return VecDeque::new();
                }
                self.check_prevote_quorum(round, leader_fn)
            }
        }
    }

    fn handle_timeout_prevote(&mut self, round: u32) -> VecDeque<StateMachineEvent> {
        if self.step != Step::Prevote || round != self.round {
            return VecDeque::new();
        };
        self.step = Step::Precommit;
        VecDeque::from([StateMachineEvent::Precommit(None, round)])
    }

    // A precommit from a peer (or self) node.
    fn handle_precommit<LeaderFn>(
        &mut self,
        block_hash: Option<BlockHash>,
        round: u32,
        leader_fn: &LeaderFn,
    ) -> VecDeque<StateMachineEvent>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        let precommit_count =
            self.precommits.entry(round).or_default().entry(block_hash).or_insert(0);
        // TODO(matan): Use variable weight.
        *precommit_count += 1;
        match round.cmp(&self.round) {
            std::cmp::Ordering::Less => self.past_round_upons(round),
            std::cmp::Ordering::Greater => self.future_round_upons(round, leader_fn),
            std::cmp::Ordering::Equal => self.check_precommit_quorum(round, leader_fn),
        }
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
        self.advance_to_round(round + 1, leader_fn)
    }

    fn advance_to_step<LeaderFn>(
        &mut self,
        step: Step,
        leader_fn: &LeaderFn,
    ) -> VecDeque<StateMachineEvent>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        self.step = step;
        // Check for an existing quorum in case messages arrived out of order.
        match self.step {
            Step::Propose => unreachable!("Advancing to Propose is done by advancing rounds"),
            Step::Prevote => self.check_prevote_quorum(self.round, leader_fn),
            Step::Precommit => self.check_precommit_quorum(self.round, leader_fn),
        }
    }

    fn check_prevote_quorum<LeaderFn>(
        &mut self,
        round: u32,
        leader_fn: &LeaderFn,
    ) -> VecDeque<StateMachineEvent>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        assert_eq!(round, self.round, "check_prevote_quorum is only called for the current round");
        let num_votes = self.prevotes.get(&round).map_or(0, |v| v.values().sum());
        if num_votes < self.quorum {
            return VecDeque::new();
        }
        let mut output = VecDeque::from([StateMachineEvent::TimeoutPrevote(round)]);
        let Some((block_hash, count)) = leading_vote(&self.prevotes, round) else {
            return output;
        };
        if *count < self.quorum {
            return output;
        }
        if block_hash.is_none() {
            output.append(&mut self.send_precommit(*block_hash, round, leader_fn));
            return output;
        }
        let Some((proposed_value, _)) = self.proposals.get(&round) else {
            return output;
        };
        if proposed_value != block_hash {
            // TODO(matan): This can be caused by a malicious leader double proposing.
            panic!("Proposal does not match quorum.");
        }

        self.locked_value = match self.locked_value {
            Some((_, locked_round)) if locked_round >= round => self.locked_value,
            _ => block_hash.map(|hash| (hash, round)),
        };

        output.append(&mut self.send_precommit(*block_hash, round, leader_fn));
        output
    }

    fn check_precommit_quorum<LeaderFn>(
        &mut self,
        round: u32,
        leader_fn: &LeaderFn,
    ) -> VecDeque<StateMachineEvent>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        let num_votes = self.precommits.get(&round).map_or(0, |v| v.values().sum());
        if num_votes < self.quorum {
            return VecDeque::new();
        }
        let mut output = VecDeque::from([StateMachineEvent::TimeoutPrecommit(round)]);
        let Some((block_hash, count)) = leading_vote(&self.precommits, round) else {
            return output;
        };
        if *count < self.quorum {
            return output;
        }
        if block_hash.is_none() {
            if round == self.round {
                output.append(&mut self.advance_to_round(round + 1, leader_fn));
                return output;
            } else {
                // NIL quorum reached on a different round.
                return output;
            }
        }
        let Some((proposed_value, _)) = self.proposals.get(&round) else {
            return output;
        };
        if proposed_value != block_hash {
            // TODO(matan): This can be caused by a malicious leader double proposing.
            panic!("Proposal does not match quorum.");
        }
        if let Some(block_hash) = block_hash {
            output.append(&mut VecDeque::from([StateMachineEvent::Decision(*block_hash, round)]));
            output
        } else {
            // NIL quorum reached on a different round.
            output
        }
    }

    fn send_precommit<LeaderFn>(
        &mut self,
        block_hash: Option<BlockHash>,
        round: u32,
        leader_fn: &LeaderFn,
    ) -> VecDeque<StateMachineEvent>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        let mut output = VecDeque::from([StateMachineEvent::Precommit(block_hash, round)]);
        output.append(&mut self.advance_to_step(Step::Precommit, leader_fn));
        output
    }

    fn advance_to_round<LeaderFn>(
        &mut self,
        round: u32,
        leader_fn: &LeaderFn,
    ) -> VecDeque<StateMachineEvent>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        self.round = round;
        self.step = Step::Propose;
        if self.id == leader_fn(self.round) {
            match self.locked_value {
                Some((proposal_content_id, valid_round)) => {
                    return VecDeque::from([StateMachineEvent::Proposal(
                        Some(proposal_content_id),
                        self.round,
                        Some(valid_round),
                    )]);
                }
                None => {
                    self.awaiting_get_proposal = true;
                    return VecDeque::from([StateMachineEvent::GetProposal(None, self.round)]);
                }
            }
        }
        let Some((proposal, _)) = self.proposals.get(&round) else {
            return VecDeque::from([StateMachineEvent::TimeoutPropose(round)]);
        };
        self.process_proposal(*proposal, round, leader_fn)
    }

    fn past_round_upons(&mut self, round: u32) -> VecDeque<StateMachineEvent> {
        let mut output = VecDeque::new();
        output.append(&mut self.upon_reproposal());
        output.append(&mut self.upon_decision(round));
        output
    }

    fn future_round_upons<LeaderFn>(
        &mut self,
        round: u32,
        leader_fn: &LeaderFn,
    ) -> VecDeque<StateMachineEvent>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        let num_prevotes = self.prevotes.get(&round).map_or(0, |v| v.values().sum());
        let num_precommits = self.precommits.get(&round).map_or(0, |v| v.values().sum());
        if num_prevotes >= self.round_skip_threshold || num_precommits >= self.round_skip_threshold
        {
            self.future_round_vote(round, leader_fn)
        } else {
            VecDeque::new()
        }
    }

    // LOC 55 in the paper.
    fn future_round_vote<LeaderFn>(
        &mut self,
        round: u32,
        leader_fn: &LeaderFn,
    ) -> VecDeque<StateMachineEvent>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        self.advance_to_round(round, leader_fn)
    }

    // LOC 28 in the paper.
    fn upon_reproposal(&mut self) -> VecDeque<StateMachineEvent> {
        if self.step != Step::Propose {
            return VecDeque::new();
        }
        let Some((block_hash, valid_round)) = self.proposals.get(&self.round) else {
            return VecDeque::new();
        };
        let Some(valid_round) = valid_round else {
            return VecDeque::new();
        };
        if valid_round >= &self.round {
            return VecDeque::new();
        }
        let Some(round_prevotes) = self.prevotes.get(valid_round) else {
            return VecDeque::new();
        };
        let Some(count) = round_prevotes.get(block_hash) else { return VecDeque::new() };

        if count < &self.quorum {
            return VecDeque::new();
        }
        let output = if block_hash.is_some_and(|v| {
            self.locked_value.is_none()
                || self.locked_value.is_some_and(|(locked_value, locked_round)| {
                    locked_round <= *valid_round || locked_value == v
                })
        }) {
            VecDeque::from([StateMachineEvent::Prevote(*block_hash, self.round)])
        } else {
            VecDeque::from([StateMachineEvent::Prevote(None, self.round)])
        };

        self.step = Step::Prevote;
        output
    }

    // LOC 49 in the paper.
    fn upon_decision(&mut self, round: u32) -> VecDeque<StateMachineEvent> {
        let Some((block_hash, count)) = leading_vote(&self.precommits, round) else {
            return VecDeque::new();
        };
        if *count < self.quorum {
            return VecDeque::new();
        }
        let Some(block_hash) = block_hash else {
            return VecDeque::new();
        };
        let Some((proposed_value, _)) = self.proposals.get(&round) else {
            return VecDeque::new();
        };
        if *proposed_value != Some(*block_hash) {
            // If the proposal is None this could be due to an honest error (crash or network).
            // If the proposal is valid, this can be caused by a malicious leader double proposing.
            // We will rely on the sync protocol to catch us up if such a decision is reached.
            error!("Proposal does not match quorum.");
            return VecDeque::new();
        }
        VecDeque::from([StateMachineEvent::Decision(*block_hash, round)])
    }
}

fn leading_vote(
    votes: &HashMap<u32, HashMap<Option<BlockHash>, u32>>,
    round: u32,
) -> Option<(&Option<BlockHash>, &u32)> {
    // We don't care which value is chosen in the case of a tie, since consensus requires 2/3+1.
    votes.get(&round)?.iter().max_by(|a, b| a.1.cmp(b.1))
}
