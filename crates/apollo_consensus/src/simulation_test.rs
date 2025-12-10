//! Discrete event simulation test for consensus protocol.
//!
//! This test uses a discrete event simulation approach with a timeline-based
//! event queue.
//!
//! Messages are scheduled with random delays to simulate network jitter.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

use apollo_consensus_config::config::TimeoutsConfig;
use apollo_protobuf::consensus::{ProposalInit, Vote, VoteType};
use lazy_static::lazy_static;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use starknet_api::block::BlockNumber;
use starknet_types_core::felt::Felt;

use crate::single_height_consensus::SingleHeightConsensus;
use crate::state_machine::{SMRequest, StateMachineEvent, Step};
use crate::types::{Decision, ProposalCommitment, Round, ValidatorId};
use crate::votes_threshold::QuorumType;

const HEIGHT_0: BlockNumber = BlockNumber(0);
const PROPOSAL_COMMITMENT: ProposalCommitment = ProposalCommitment(Felt::ONE);
const TOTAL_NODES: usize = 100;
const THRESHOLD: usize = (2 * TOTAL_NODES / 3) + 1;
const DEADLINE_TICKS: u64 = 200;
const NODE_0_LEADER_PROBABILITY: f64 = 0.1;

lazy_static! {
    static ref NODE_0: ValidatorId = ValidatorId::from(0u64);
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
    /// The seed used to initialize the simulation.
    seed: u64,
    /// The single height consensus instance.
    shc: SingleHeightConsensus,
    /// All validators in the network.
    validators: Vec<ValidatorId>,
    /// The current round being processed.
    current_round: Option<Round>,
    /// Priority queue of pending events that have yet to be processed (min-heap by tick).
    pending_events: BinaryHeap<TimedEvent>,
    /// Current simulation tick.
    current_tick: u64,
    /// History of all processed events.
    processed_history: Vec<InputEvent>,
    /// Tracks build finish delay and commitment for each round.
    /// Key: round, Value: (build_finish_delay, proposal_commitment)
    build_finish_info: HashMap<Round, (u64, Option<ProposalCommitment>)>,
    /// Tracks what the node actually voted for in each round.
    node_votes: HashMap<Round, Vote>,
}

impl DiscreteEventSimulation {
    fn new(total_nodes: usize, seed: u64) -> Self {
        let rng = StdRng::seed_from_u64(seed);
        let validators: Vec<ValidatorId> =
            (0..total_nodes).map(|i| ValidatorId::from(u64::try_from(i).unwrap())).collect();

        let shc = SingleHeightConsensus::new(
            HEIGHT_0,
            false,
            *NODE_0,
            validators.clone(),
            QuorumType::Byzantine,
            TimeoutsConfig::default(),
        );

        Self {
            rng,
            seed,
            shc,
            validators,
            current_round: None,
            pending_events: BinaryHeap::new(),
            current_tick: 0,
            processed_history: Vec::new(),
            build_finish_info: HashMap::new(),
            node_votes: HashMap::new(),
        }
    }

    /// Probabilistically selects a leader for the given round.
    /// Node 0 (the one under test) has probability NODE_0_LEADER_PROBABILITY of being selected.
    /// Other nodes share the remaining probability (1 - NODE_0_LEADER_PROBABILITY) uniformly.
    /// The selection is deterministic per round - the same round will always return the same
    /// leader.
    fn get_leader(seed: u64, round: Round) -> ValidatorId {
        let round_u64 = u64::from(round);
        let round_seed = seed.wrapping_mul(31).wrapping_add(round_u64);
        let mut round_rng = StdRng::seed_from_u64(round_seed);

        let random_value: f64 = round_rng.gen();

        if random_value < NODE_0_LEADER_PROBABILITY {
            *NODE_0
        } else {
            let idx = round_rng.gen_range(1..TOTAL_NODES);
            ValidatorId::from(u64::try_from(idx).unwrap())
        }
    }

    /// Schedules an event to occur after the specified delay.
    fn schedule(&mut self, delay: u64, event: InputEvent) {
        self.pending_events.push(TimedEvent { tick: self.current_tick + delay, event });
    }

    /// Generates traffic for a specific round with only honest nodes.
    ///
    /// - Proposer sends: Proposal -> Prevote -> Precommit (in order)
    /// - Other validators send: Prevote -> Precommit (in order)
    ///
    /// Messages are scheduled with random delays to simulate network jitter,
    /// but each node's messages maintain correct ordering.
    fn generate_round_traffic(&mut self, round: Round) {
        let leader_id = Self::get_leader(self.seed, round);

        // 1. Proposal from leader (if not self)
        if leader_id != *NODE_0 {
            let delay = self.rng.gen_range(2..20);
            self.schedule(
                delay,
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
            // TODO(Asmaa): currently ignore the proposal commitment value, since we are in the
            // honest network. Update this once we add invalid votes: enable invalid votes to be
            // sent anytime.
            let base = self.build_finish_info.get(&round).map(|(delay, _)| *delay).unwrap_or(0);
            let prevote_delay = base + self.rng.gen_range(2..20);
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

    fn check_and_generate_next_round(&mut self) {
        if self.current_round.is_none() || self.current_round.unwrap() < self.shc.current_round() {
            self.current_round = Some(self.shc.current_round());
            self.generate_round_traffic(self.shc.current_round());
        }
    }

    /// Runs the simulation until a decision is reached or the deadline is exceeded.
    ///
    /// Returns `Some(Decision)` if consensus is reached, `None` if the deadline
    /// is reached without a decision.
    fn run(&mut self, deadline_ticks: u64) -> Option<Decision> {
        let seed = self.seed;
        let leader_fn = move |r: Round| Self::get_leader(seed, r);

        // Start the single height consensus
        let requests = self.shc.start(&leader_fn);
        if let Some(decision) = self.handle_requests(requests) {
            return Some(decision);
        }

        // Main event loop
        while let Some(timed_event) = self.pending_events.pop() {
            if timed_event.tick > deadline_ticks {
                break;
            }

            self.current_tick = timed_event.tick;
            self.processed_history.push(timed_event.event.clone());

            // Process the event
            let requests = match timed_event.event {
                InputEvent::Vote(v) => self.shc.handle_vote(&leader_fn, v),
                InputEvent::Proposal(p) => self.shc.handle_proposal(&leader_fn, p),
                InputEvent::Internal(e) => self.shc.handle_event(&leader_fn, e),
            };

            if let Some(decision) = self.handle_requests(requests) {
                return Some(decision);
            }
        }

        None
    }

    /// Handles state machine requests by scheduling appropriate events.
    ///
    /// This simulates the manager's role in handling consensus requests,
    /// such as validation results, proposal building, and timeouts.
    fn handle_requests(&mut self, reqs: VecDeque<SMRequest>) -> Option<Decision> {
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
                    self.build_finish_info.insert(round, (delay, Some(PROPOSAL_COMMITMENT)));
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
                SMRequest::BroadcastVote(vote) => {
                    self.node_votes.insert(vote.round, vote);
                }
                SMRequest::DecisionReached(decision) => {
                    return Some(decision);
                }
                _ => {
                    // Ignore other request types
                }
            }
            self.check_and_generate_next_round();
        }
        None
    }
}

fn verify_result(sim: &DiscreteEventSimulation, result: Option<&Decision>) {
    #[derive(Default)]
    struct RoundStats {
        peer_precommits: usize,
        finished_proposal: bool,
        expected_commitment: ProposalCommitment,
    }

    let mut stats: HashMap<Round, RoundStats> = HashMap::new();

    // Aggregate stats from the processed history.
    for event in &sim.processed_history {
        match event {
            // Track peer precommits.
            InputEvent::Vote(v) => {
                if v.vote_type == VoteType::Precommit {
                    if let Some(proposal_commitment) = v.proposal_commitment {
                        let entry = stats.entry(v.round).or_insert_with(|| RoundStats {
                            expected_commitment: PROPOSAL_COMMITMENT,
                            ..Default::default()
                        });
                        if proposal_commitment == entry.expected_commitment {
                            entry.peer_precommits += 1;
                        }
                    }
                }
            }
            // Track proposal knowledge.
            InputEvent::Internal(StateMachineEvent::FinishedValidation(c, r, _))
            | InputEvent::Internal(StateMachineEvent::FinishedBuilding(c, r)) => {
                if let Some(proposal_commitment) = *c {
                    let entry = stats.entry(*r).or_insert_with(|| RoundStats {
                        expected_commitment: PROPOSAL_COMMITMENT,
                        ..Default::default()
                    });
                    if proposal_commitment == entry.expected_commitment {
                        entry.finished_proposal = true;
                    }
                }
            }
            _ => {}
        }
    }

    // 2. Determine Expected Decision
    // Use the actual votes the node broadcast (from BroadcastVote requests)
    // instead of inferring from timeouts
    let mut expected_decision: Option<(Round, ProposalCommitment)> = None;

    for r in 0..=sim.current_round.unwrap() {
        if let Some(s) = stats.get(&r) {
            // Check what the node actually voted for in this round
            // If the node voted precommit for the valid commitment, count it
            let expected_commitment = PROPOSAL_COMMITMENT;
            let self_vote = sim.node_votes.get(&r).map_or(0, |v| {
                if v.vote_type == VoteType::Precommit
                    && v.proposal_commitment == Some(expected_commitment)
                {
                    1
                } else {
                    0
                }
            });

            let total_precommits = s.peer_precommits + self_vote;

            if s.finished_proposal && total_precommits >= THRESHOLD {
                expected_decision = Some((r, expected_commitment));
                break;
            }
        }
    }

    // 3. Compare with Actual Result
    match (result, expected_decision) {
        (Some(actual), Some((expected_round, expected_commitment))) => {
            let decided_round = actual.precommits[0].round;
            let decided_block = actual.block;
            assert_eq!(
                decided_block, expected_commitment,
                "Decision block mismatch. History: {:?}",
                sim.processed_history
            );
            assert_eq!(
                decided_round, expected_round,
                "Decision round mismatch expected: {:?}, actual: {:?}. History: {:?}",
                expected_round, decided_round, sim.processed_history
            );

            // 4. Verify that decision precommits are all in history (or are the node's own vote)
            // Collect all precommits from processed_history
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

            let decision_precommits: HashSet<_> = actual.precommits.iter().cloned().collect();

            // Check that the difference (decision precommits not in history) is only the node's
            // vote
            let diff: HashSet<_> =
                decision_precommits.difference(&history_precommits).cloned().collect();
            // Get the node's own vote (if it voted for this round/proposal)
            let expected_diff = sim.node_votes.get(&decided_round).map_or(HashSet::new(), |v| {
                if v.vote_type == VoteType::Precommit
                    && v.proposal_commitment == Some(decided_block)
                {
                    HashSet::from([v.clone()])
                } else {
                    HashSet::new()
                }
            });
            assert_eq!(
                diff, expected_diff,
                "Decision has precommits not in history that don't match node vote. Diff: {:?}, \
                 History: {:?}, Decision: {:?}",
                diff, history_precommits, actual.precommits
            );

            // Verify quorum threshold is met
            assert!(
                actual.precommits.len() >= THRESHOLD,
                "Insufficient precommits in decision: {}/{}. Decision: {:?}, History: {:?}",
                actual.precommits.len(),
                THRESHOLD,
                actual,
                sim.processed_history
            );
        }
        (None, None) => {
            // SUCCESS: No decision reached. History confirms conditions were never met.
        }
        _ => {
            panic!(
                "FAILURE: returned {result:?}, expected {expected_decision:?}. History: {:?}",
                sim.processed_history
            );
        }
    }
}

#[test]
fn test_honest_nodes_only() {
    let seed = rand::thread_rng().gen();
    println!("Running consensus simulation with total nodes {TOTAL_NODES} and seed: {seed}");

    let mut sim = DiscreteEventSimulation::new(TOTAL_NODES, seed);

    let result = sim.run(DEADLINE_TICKS);

    verify_result(&sim, result.as_ref());
}
