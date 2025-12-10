//! Discrete event simulation test for consensus protocol.
//!
//! This test uses a discrete event simulation approach with a timeline-based
//! event queue.
//!
//! Messages are scheduled with random delays to simulate network jitter.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashSet, VecDeque};

use apollo_consensus_config::config::TimeoutsConfig;
use apollo_protobuf::consensus::{ProposalInit, Vote, VoteType};
use lazy_static::lazy_static;
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

lazy_static! {
    static ref VALIDATOR_ID: ValidatorId = ValidatorId::from(0u64);
}

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
/// Events are ordered by ascending tick (earliest first).
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

        let shc = SingleHeightConsensus::new(
            HEIGHT_0,
            false,
            *VALIDATOR_ID,
            validators.clone(),
            QuorumType::Byzantine,
            TimeoutsConfig::default(),
        );

        Self {
            rng,
            shc,
            validators,
            timeline: BinaryHeap::new(),
            current_tick: 0,
            processed_history: Vec::new(),
        }
    }

    fn get_leader(round: Round) -> ValidatorId {
        let round_u64 = u64::from(round);
        let hash = SIMULATION_SEED.wrapping_mul(31).wrapping_add(round_u64);
        let idx = hash % u64::try_from(TOTAL_NODES).unwrap();
        ValidatorId::from(idx)
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
        let leader_id = Self::get_leader(round);

        // 1. Proposal from leader (if not self)
        if leader_id != *VALIDATOR_ID {
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
            let prevote_delay = self.rng.gen_range(2..20);
            let precommit_delta = self.rng.gen_range(5..20);

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
        let leader_fn = |r: Round| Self::get_leader(r);

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
                    let delay = self.rng.gen_range(15..30);
                    let result = StateMachineEvent::FinishedValidation(
                        Some(PROPOSAL_COMMITMENT),
                        init.round,
                        None,
                    );
                    self.schedule(delay, InputEvent::Internal(result));
                }
                SMRequest::StartBuildProposal(round) => {
                    let delay = self.rng.gen_range(15..30);
                    let result =
                        StateMachineEvent::FinishedBuilding(Some(PROPOSAL_COMMITMENT), round);
                    self.schedule(delay, InputEvent::Internal(result));
                }
                SMRequest::ScheduleTimeout(step, round) => {
                    let (delay, event) = match step {
                        Step::Propose => {
                            (self.rng.gen_range(15..30), StateMachineEvent::TimeoutPropose(round))
                        }
                        Step::Prevote => {
                            (self.rng.gen_range(5..10), StateMachineEvent::TimeoutPrevote(round))
                        }
                        Step::Precommit => {
                            (self.rng.gen_range(5..10), StateMachineEvent::TimeoutPrecommit(round))
                        }
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
    let decision = result.unwrap_or_else(|| {
        panic!(
            "FAILURE: Simulation timed out! Honest network should always decide. History: {:?}",
            sim.processed_history
        )
    });

    let decided_block = decision.block;
    let decided_round = decision.precommits[0].round;

    // 1. Verify correct block commitment
    assert_eq!(
        decided_block, PROPOSAL_COMMITMENT,
        "Block commitment mismatch. History: {:?}",
        sim.processed_history
    );

    // 2. Verify decision in round 0 (honest network should decide immediately)
    assert_eq!(
        decided_round, 0,
        "Honest network should decide in Round 0. History: {:?}",
        sim.processed_history
    );

    // 3. Verify that decision has the same precommits as history.
    let history_precommits: HashSet<_> = sim
        .processed_history
        .iter()
        .filter_map(|e| {
            if let InputEvent::Vote(v) = e {
                if v.vote_type == VoteType::Precommit
                    && v.round == decided_round
                    && v.proposal_commitment == Some(decided_block)
                {
                    Some(v.clone())
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    let decision_precommits: HashSet<_> = decision.precommits.iter().cloned().collect();

    // Decision should contain all history precommits, plus possibly the self vote
    assert!(
        history_precommits.is_subset(&decision_precommits),
        "Decision precommits don't contain all history precommits. Decision: {:?}, History: {:?}",
        decision,
        sim.processed_history
    );

    // Decision should have at most one extra vote (the self vote)
    let extra_votes = decision_precommits.difference(&history_precommits).count();
    assert!(
        extra_votes <= 1,
        "Decision has {} extra precommits, expected at most 1 (self vote). Decision: {:?}, \
         History: {:?}",
        extra_votes,
        decision,
        sim.processed_history
    );

    // Verify quorum threshold is met
    assert!(
        decision.precommits.len() >= THRESHOLD,
        "Insufficient precommits in decision: {}/{}. Decision: {:?}, History: {:?}",
        decision.precommits.len(),
        THRESHOLD,
        decision,
        sim.processed_history
    );
}

#[test]
fn test_honest_nodes_only() {
    let mut sim = DiscreteEventSimulation::new(TOTAL_NODES, SIMULATION_SEED);

    sim.generate_round_traffic(0);

    let result = sim.run(DEADLINE_TICKS);

    verify_honest_success(&sim, result.as_ref());
}
