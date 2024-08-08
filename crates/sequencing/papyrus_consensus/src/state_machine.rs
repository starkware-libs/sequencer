//! State machine for Starknet consensus.
//!
//! LOC refers to the line of code from Algorithm 1 (page 6) of the tendermint
//! [paper](https://arxiv.org/pdf/1807.04938).

#[cfg(test)]
#[path = "state_machine_test.rs"]
mod state_machine_test;

use std::collections::{HashMap, VecDeque};

use starknet_api::block::BlockHash;
use tracing::trace;

use crate::types::{Round, ValidatorId};

/// Events which the state machine sends/receives.
#[derive(Debug, Clone, PartialEq)]
pub enum StateMachineEvent {
    /// Sent by the state machine when a block is required to propose (BlockHash is always None).
    /// While waiting for the response of GetProposal, the state machine will buffer all other
    /// events. The caller must respond with a valid block hash for this height to the state
    /// machine, and the same round sent out.
    GetProposal(Option<BlockHash>, Round),
    /// Consensus message, can be both sent from and to the state machine.
    Proposal(Option<BlockHash>, Round),
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
    proposals: HashMap<Round, Option<BlockHash>>,
    // {round: {block_hash: vote_count}
    prevotes: HashMap<Round, HashMap<Option<BlockHash>, u32>>,
    precommits: HashMap<Round, HashMap<Option<BlockHash>, u32>>,
    // When true, the state machine will wait for a GetProposal event, buffering all other input
    // events in `events_queue`.
    awaiting_get_proposal: bool,
    events_queue: VecDeque<StateMachineEvent>,
}

impl StateMachine {
    /// total_weight - the total voting weight of all validators for this height.
    pub fn new(id: ValidatorId, total_weight: u32) -> Self {
        Self {
            id,
            round: 0,
            step: Step::Propose,
            quorum: (2 * total_weight / 3) + 1,
            proposals: HashMap::new(),
            prevotes: HashMap::new(),
            precommits: HashMap::new(),
            awaiting_get_proposal: false,
            events_queue: VecDeque::new(),
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

        // The events queue only maintains state while we are waiting for a proposal.
        let events_queue = std::mem::take(&mut self.events_queue);
        self.handle_enqueued_events(events_queue, leader_fn)
    }

    fn handle_enqueued_events<LeaderFn>(
        &mut self,
        mut events_queue: VecDeque<StateMachineEvent>,
        leader_fn: &LeaderFn,
    ) -> VecDeque<StateMachineEvent>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        let mut output_events = VecDeque::new();
        while let Some(event) = events_queue.pop_front() {
            // Handle a specific event and then decide which of the output events should also be
            // sent to self.
            for e in self.handle_event_internal(event, leader_fn) {
                match e {
                    StateMachineEvent::Proposal(_, _)
                    | StateMachineEvent::Prevote(_, _)
                    | StateMachineEvent::Precommit(_, _) => {
                        events_queue.push_back(e.clone());
                    }
                    StateMachineEvent::Decision(_, _) => {
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
        match event {
            StateMachineEvent::GetProposal(block_hash, round) => {
                self.handle_get_proposal(block_hash, round)
            }
            StateMachineEvent::Proposal(block_hash, round) => {
                self.handle_proposal(block_hash, round, leader_fn)
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
        VecDeque::from([StateMachineEvent::Proposal(block_hash, round)])
    }

    // A proposal from a peer (or self) node.
    fn handle_proposal<LeaderFn>(
        &mut self,
        block_hash: Option<BlockHash>,
        round: u32,
        leader_fn: &LeaderFn,
    ) -> VecDeque<StateMachineEvent>
    where
        LeaderFn: Fn(Round) -> ValidatorId,
    {
        let old = self.proposals.insert(round, block_hash);
        assert!(old.is_none(), "SHC should handle conflicts & replays");
        if round != self.round {
            return VecDeque::new();
        }
        self.process_proposal(block_hash, round, leader_fn)
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

        if self.step != Step::Prevote || round != self.round {
            return VecDeque::new();
        }
        self.check_prevote_quorum(round, leader_fn)
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

        self.check_precommit_quorum(round, leader_fn)
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
        let mut output = if num_votes < self.quorum {
            return VecDeque::new();
        } else {
            VecDeque::from([StateMachineEvent::TimeoutPrevote(round)])
        };

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
        let Some(proposed_value) = self.proposals.get(&round) else {
            return output;
        };
        if proposed_value != block_hash {
            // TODO(matan): This can be caused by a malicious leader double proposing.
            panic!("Proposal does not match quorum.");
        }

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
        let mut output = if num_votes < self.quorum {
            return VecDeque::new();
        } else {
            VecDeque::from([StateMachineEvent::TimeoutPrecommit(round)])
        };
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
        let Some(proposed_value) = self.proposals.get(&round) else {
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
            self.awaiting_get_proposal = true;
            // TODO(matan): Support re-proposing validValue.
            return VecDeque::from([StateMachineEvent::GetProposal(None, self.round)]);
        }
        let Some(proposal) = self.proposals.get(&round) else {
            return VecDeque::from([StateMachineEvent::TimeoutPropose(round)]);
        };
        self.process_proposal(*proposal, round, leader_fn)
    }
}

fn leading_vote(
    votes: &HashMap<u32, HashMap<Option<BlockHash>, u32>>,
    round: u32,
) -> Option<(&Option<BlockHash>, &u32)> {
    // We don't care which value is chosen in the case of a tie, since consensus requires 2/3+1.
    votes.get(&round)?.iter().max_by(|a, b| a.1.cmp(b.1))
}
