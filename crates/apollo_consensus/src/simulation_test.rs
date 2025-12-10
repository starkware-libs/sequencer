//! Discrete event simulation test for consensus protocol.
//!
//! This test uses a discrete event simulation approach with a timeline-based
//! event queue.
//!
//! Messages are scheduled with random delays to simulate network jitter.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, VecDeque};

use apollo_consensus_config::config::TimeoutsConfig;
use apollo_protobuf::consensus::{ProposalInit, Vote, VoteType};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use starknet_api::block::BlockNumber;
use starknet_types_core::felt::Felt;

use crate::single_height_consensus::{ShcReturn, SingleHeightConsensus};
use crate::state_machine::{SMRequest, StateMachineEvent, Step};
use crate::types::{Decision, ProposalCommitment, Round, ValidatorId};
use crate::votes_threshold::QuorumType;

const HEIGHT_0: BlockNumber = BlockNumber(0);
const PROPOSAL_COMMITMENT: ProposalCommitment = ProposalCommitment(Felt::ONE);
const TOTAL_NODES: usize = 100;
const THRESHOLD: usize = (2 * TOTAL_NODES / 3) + 1;
const SIMULATION_SEED: u64 = 100;
const DEADLINE_TICKS: u64 = 200;

/// Represents an input event in the simulation.
#[derive(Debug, Clone)]
enum InputEvent {
    /// A vote message from peer node.
    Vote(Vote),
    /// A proposal message.
    Proposal(ProposalInit),
    /// An internal event.
    Internal(StateMachineEvent),
}

/// A timed event in the discrete event simulation.
///
/// Events are ordered by tick (time) in reverse order for the priority queue
/// (earliest events have highest priority).
#[derive(Debug)]
struct TimedEvent {
    /// The simulation tick at which this event should occur.
    tick: u64,
    /// The event to process.
    event: InputEvent,
}

impl PartialEq for TimedEvent {
    fn eq(&self, other: &Self) -> bool {
        self.tick == other.tick
    }
}

impl Eq for TimedEvent {}

impl PartialOrd for TimedEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TimedEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        other.tick.cmp(&self.tick)
    }
}

/// Discrete event simulation for consensus protocol.
///
/// Uses a timeline-based approach where events are scheduled at specific
/// ticks and processed in chronological order.
struct DiscreteEventSimulation {
    /// Random number generator for scheduling delays.
    rng: StdRng,
    /// The ID of the node being simulated (node 0).
    node_id: ValidatorId,
    /// The single height consensus instance.
    shc: SingleHeightConsensus,
    /// All validators in the network.
    validators: Vec<ValidatorId>,
    /// Priority queue of timed events (min-heap by tick).
    timeline: BinaryHeap<TimedEvent>,
    /// Current simulation tick.
    current_tick: u64,
    /// History of all processed events.
    processed_history: Vec<InputEvent>,
}

impl DiscreteEventSimulation {
    fn new(total_nodes: usize, seed: u64) -> Self {
        let rng = StdRng::seed_from_u64(seed);
        let validators: Vec<ValidatorId> =
            (0..total_nodes).map(|i| ValidatorId::from(u64::try_from(i).unwrap())).collect();
        let node_id = validators[0];

        let shc = SingleHeightConsensus::new(
            HEIGHT_0,
            false,
            node_id,
            validators.clone(),
            QuorumType::Byzantine,
            TimeoutsConfig::default(),
        );

        Self {
            rng,
            node_id,
            shc,
            validators,
            timeline: BinaryHeap::new(),
            current_tick: 0,
            processed_history: Vec::new(),
        }
    }

    fn get_leader(&self, round: Round) -> ValidatorId {
        let idx = usize::try_from(round).unwrap() % self.validators.len();
        self.validators[idx]
    }

    /// Schedules an event to occur after the specified delay.
    fn schedule(&mut self, delay: u64, event: InputEvent) {
        self.timeline.push(TimedEvent { tick: self.current_tick + delay, event });
    }

    /// Generates traffic for a specific round with only honest nodes.
    ///
    /// - Proposer sends: Proposal -> Prevote -> Precommit (in order)
    /// - Other validators send: Prevote -> Precommit (in order)
    ///
    /// Messages are scheduled with random delays to simulate network jitter,
    /// but each node's messages maintain correct ordering.
    fn generate_round_traffic(&mut self, round: Round) {
        let leader_id = self.get_leader(round);

        // 1. Proposal from leader (if not self)
        if leader_id != self.node_id {
            self.schedule(
                1,
                InputEvent::Proposal(ProposalInit {
                    height: HEIGHT_0,
                    round,
                    proposer: leader_id,
                    valid_round: None,
                }),
            );
        }

        // 2. Votes from other honest validators
        // Skip index 0 (self) - our votes are handled by the state machine
        for i in 1..self.validators.len() {
            let voter = self.validators[i];
            let commitment = Some(PROPOSAL_COMMITMENT);

            // Random delays to simulate network jitter
            let prevote_delay = self.rng.gen_range(2..10);
            let precommit_delta = self.rng.gen_range(5..10);

            // Schedule prevote
            self.schedule(
                prevote_delay,
                InputEvent::Vote(Vote {
                    vote_type: VoteType::Prevote,
                    height: HEIGHT_0,
                    round,
                    proposal_commitment: commitment,
                    voter,
                }),
            );

            // Schedule precommit (after prevote)
            self.schedule(
                prevote_delay + precommit_delta,
                InputEvent::Vote(Vote {
                    vote_type: VoteType::Precommit,
                    height: HEIGHT_0,
                    round,
                    proposal_commitment: commitment,
                    voter,
                }),
            );
        }
    }

    /// Runs the simulation until a decision is reached or the deadline is exceeded.
    ///
    /// Returns `Some(Decision)` if consensus is reached, `None` if the deadline
    /// is reached without a decision.
    fn run(&mut self, deadline_ticks: u64) -> Option<Decision> {
        let validators = self.validators.clone();
        let leader_fn = |r: Round| {
            let idx = usize::try_from(r).unwrap() % validators.len();
            validators[idx]
        };

        // Start the single height consensus
        match self.shc.start(&leader_fn) {
            ShcReturn::Decision(d) => return Some(d),
            ShcReturn::Requests(reqs) => self.handle_requests(reqs),
        }

        // Main event loop
        while let Some(timed_event) = self.timeline.pop() {
            if timed_event.tick > deadline_ticks {
                break;
            }

            self.current_tick = timed_event.tick;
            self.processed_history.push(timed_event.event.clone());

            // Process the event
            let res = match timed_event.event {
                InputEvent::Vote(v) => self.shc.handle_vote(&leader_fn, v),
                InputEvent::Proposal(p) => self.shc.handle_proposal(&leader_fn, p),
                InputEvent::Internal(e) => self.shc.handle_event(&leader_fn, e),
            };

            match res {
                ShcReturn::Decision(d) => return Some(d),
                ShcReturn::Requests(reqs) => self.handle_requests(reqs),
            }
        }

        None
    }

    /// Handles state machine requests by scheduling appropriate events.
    ///
    /// This simulates the manager's role in handling consensus requests,
    /// such as validation results, proposal building, and timeouts.
    fn handle_requests(&mut self, reqs: VecDeque<SMRequest>) {
        for req in reqs {
            match req {
                SMRequest::StartValidateProposal(init) => {
                    let delay = self.rng.gen_range(5..10);
                    let result = StateMachineEvent::FinishedValidation(
                        Some(PROPOSAL_COMMITMENT),
                        init.round,
                        None,
                    );
                    self.schedule(delay, InputEvent::Internal(result));
                }
                SMRequest::StartBuildProposal(round) => {
                    let delay = self.rng.gen_range(5..10);
                    let result =
                        StateMachineEvent::FinishedBuilding(Some(PROPOSAL_COMMITMENT), round);
                    self.schedule(delay, InputEvent::Internal(result));
                }
                SMRequest::ScheduleTimeout(step, round) => {
                    let delay = self.rng.gen_range(5..10);
                    let event = match step {
                        Step::Propose => StateMachineEvent::TimeoutPropose(round),
                        Step::Prevote => StateMachineEvent::TimeoutPrevote(round),
                        Step::Precommit => StateMachineEvent::TimeoutPrecommit(round),
                    };
                    self.schedule(delay, InputEvent::Internal(event));
                }
                _ => {
                    // Ignore other request types
                }
            }
        }
    }
}

/// Verifies that the simulation reached a valid decision with honest nodes.
///
/// Checks:
/// - Decision was reached (not None)
/// - Correct block commitment
/// - Decision in round 0
/// - Quorum threshold is met
fn verify_honest_success(sim: &DiscreteEventSimulation, result: Option<&Decision>) {
    let decision =
        result.expect("FAILURE: Simulation timed out! Honest network should always decide.");

    let decided_block = decision.block;
    let decided_round = decision.precommits[0].round;

    // 1. Verify correct block commitment
    assert_eq!(decided_block, PROPOSAL_COMMITMENT);

    // 2. Verify decision in round 0 (honest network should decide immediately)
    assert_eq!(decided_round, 0, "Honest network should decide in Round 0");

    // 3. Verify quorum threshold is met
    let valid_votes = sim
        .processed_history
        .iter()
        .filter(|e| {
            if let InputEvent::Vote(v) = e {
                v.vote_type == VoteType::Precommit
                    && v.round == decided_round
                    && v.proposal_commitment == Some(decided_block)
            } else {
                false
            }
        })
        .count();

    // Add 1 for self (our vote is tracked internally by the state machine)
    let total = valid_votes + 1;
    assert!(total >= THRESHOLD, "Insufficient votes: {}/{}", total, THRESHOLD);
}

#[test]
fn test_honest_nodes_only() {
    let mut sim = DiscreteEventSimulation::new(TOTAL_NODES, SIMULATION_SEED);

    sim.generate_round_traffic(0);

    let result = sim.run(DEADLINE_TICKS);

    verify_honest_success(&sim, result.as_ref());
}
