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
use test_case::test_case;

use crate::single_height_consensus::{ShcReturn, SingleHeightConsensus};
use crate::state_machine::{SMRequest, StateMachineEvent, Step};
use crate::types::{Decision, ProposalCommitment, Round, ValidatorId};
use crate::votes_threshold::QuorumType;

const HEIGHT_0: BlockNumber = BlockNumber(0);
const TOTAL_NODES: usize = 100;
const THRESHOLD: usize = (2 * TOTAL_NODES / 3) + 1;
const DEADLINE_TICKS: u64 = 200;
const NODE_0_LEADER_PROBABILITY: f64 = 0.1;

lazy_static! {
    static ref VALIDATOR_ID: ValidatorId = ValidatorId::from(0u64);
}

/// Types of faulty behavior that nodes can exhibit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FaultType {
    /// Sends no messages (Offline).
    Offline,
    /// Votes for `None` (Nil).
    NilVoter,
    /// Sends two conflicting votes for the same round.
    Equivocator,
    /// Sends a proposal even when it is NOT their turn to be leader.
    UnauthorizedProposer,
    /// Sends votes with a voter ID that is not in the validator set.
    NonValidator,
}

/// Represents an input event in the simulation.
#[derive(Debug, Clone)]
enum InputEvent {
    /// A vote message from peer node, and a flag if we should count it for the quorum.
    Vote(Vote, bool),
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

/// Generates a deterministic commitment for the given round.
/// Each round gets a unique commitment based on the round number.
fn proposal_commitment_for_round(round: Round) -> ProposalCommitment {
    ProposalCommitment(Felt::from(u64::from(round)))
}

/// Generates a "fake" commitment for equivocation/conflict attacks.
fn fake_commitment_for_round(round: Round) -> ProposalCommitment {
    ProposalCommitment(Felt::from(u64::from(round) + 9999))
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
    /// Number of honest nodes (the rest are faulty).
    honest_nodes: usize,
    /// The current maximum round being processed.
    current_max_round: Round,
    /// Priority queue of pending events that have yet to be processed (min-heap by tick).
    pending_events: BinaryHeap<TimedEvent>,
    /// Current simulation tick.
    current_tick: u64,
    /// History of all processed events.
    processed_history: Vec<InputEvent>,
    /// Tracks what the node actually voted for in each round.
    node_votes: HashMap<Round, Vote>,
    /// The keep ratio for the network (probability that messages are not dropped).
    keep_ratio: f64,
}

impl DiscreteEventSimulation {
    fn new(total_nodes: usize, honest_nodes: usize, seed: u64, keep_ratio: f64) -> Self {
        assert!(honest_nodes <= total_nodes, "honest_nodes must be <= total_nodes");
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
            seed,
            shc,
            validators,
            honest_nodes,
            current_max_round: 0,
            pending_events: BinaryHeap::new(),
            current_tick: 0,
            processed_history: Vec::new(),
            node_votes: HashMap::new(),
            keep_ratio,
        }
    }

    /// Determines the fault type for a faulty node at a given round.
    /// Uses deterministic randomness based on node index and round.
    fn get_fault_type(&mut self, node_idx: usize, round: Round) -> FaultType {
        let node_idx_u64 = u64::try_from(node_idx).unwrap_or(0);
        let round_u64 = u64::from(round);
        let seed = self
            .seed
            .wrapping_mul(31)
            .wrapping_add(node_idx_u64)
            .wrapping_mul(31)
            .wrapping_add(round_u64);
        let mut fault_rng = StdRng::seed_from_u64(seed);

        // Randomly select a fault type
        let fault_types = [
            FaultType::Offline,
            FaultType::NilVoter,
            FaultType::Equivocator,
            FaultType::UnauthorizedProposer,
            FaultType::NonValidator,
        ];
        let idx = fault_rng.gen_range(0..fault_types.len());
        fault_types[idx]
    }

    /// Probabilistically selects a leader index for the given round.
    /// Node 0 (the one under test) has probability NODE_0_LEADER_PROBABILITY of being selected.
    /// Other nodes share the remaining probability (1 - NODE_0_LEADER_PROBABILITY) uniformly.
    /// The selection is deterministic per round - the same round will always return the same
    /// leader index.
    fn get_leader_index(seed: u64, round: Round) -> usize {
        let round_u64 = u64::from(round);
        let round_seed = seed.wrapping_mul(31).wrapping_add(round_u64);
        let mut round_rng = StdRng::seed_from_u64(round_seed);

        let random_value: f64 = round_rng.gen();

        if random_value < NODE_0_LEADER_PROBABILITY {
            0
        } else {
            round_rng.gen_range(1..TOTAL_NODES)
        }
    }

    /// Schedules an event to occur after the specified delay.
    /// Internal events are always scheduled.
    /// Other events are scheduled with probability keep_ratio.
    fn schedule(&mut self, delay: u64, event: InputEvent) {
        let should_enqueue =
            matches!(event, InputEvent::Internal(_)) || self.rng.gen_bool(self.keep_ratio);
        if should_enqueue {
            self.pending_events.push(TimedEvent { tick: self.current_tick + delay, event });
        }
    }

    /// Schedules both prevote and precommit for a voter in a round.
    /// Generates random delays internally to simulate network jitter.
    /// If `equivocate` is true, sends two conflicting votes: first with the given commitment
    /// (should_count=true), second with a fake commitment with should_count=false.
    fn schedule_prevote_and_precommit(
        &mut self,
        voter: ValidatorId,
        round: Round,
        commitment: Option<ProposalCommitment>,
        should_count: bool,
        equivocate: bool,
    ) {
        let prevote_delay = self.rng.gen_range(2..20);
        let precommit_delta = self.rng.gen_range(5..20);

        // Schedule first set of votes
        self.schedule(
            prevote_delay,
            InputEvent::Vote(
                Vote {
                    vote_type: VoteType::Prevote,
                    height: HEIGHT_0,
                    round,
                    proposal_commitment: commitment,
                    voter,
                },
                should_count,
            ),
        );
        self.schedule(
            prevote_delay + precommit_delta,
            InputEvent::Vote(
                Vote {
                    vote_type: VoteType::Precommit,
                    height: HEIGHT_0,
                    round,
                    proposal_commitment: commitment,
                    voter,
                },
                should_count,
            ),
        );

        // If equivocating, send second set with fake commitment
        if equivocate {
            let fake_commitment = Some(fake_commitment_for_round(round));
            self.schedule(
                prevote_delay + 1,
                InputEvent::Vote(
                    Vote {
                        vote_type: VoteType::Prevote,
                        height: HEIGHT_0,
                        round,
                        proposal_commitment: fake_commitment,
                        voter,
                    },
                    false, // Equivocating vote shouldn't count
                ),
            );
            self.schedule(
                prevote_delay + precommit_delta + 1,
                InputEvent::Vote(
                    Vote {
                        vote_type: VoteType::Precommit,
                        height: HEIGHT_0,
                        round,
                        proposal_commitment: fake_commitment,
                        voter,
                    },
                    false, // Equivocating vote shouldn't count
                ),
            );
        }
    }

    /// Generates traffic for a specific round with a given keep ratio.
    ///
    /// - Proposer sends: Proposal -> Prevote -> Precommit (in order)
    /// - Other validators send: Prevote -> Precommit (in order)
    ///
    /// Messages are scheduled with random delays to simulate network jitter,
    /// but each node's messages maintain correct ordering.
    fn generate_round_traffic(&mut self, round: Round) {
        let leader_idx = Self::get_leader_index(self.seed, round);
        let leader_id = self.validators[leader_idx];
        let proposal_commitment = Some(proposal_commitment_for_round(round));

        if leader_id != *VALIDATOR_ID && leader_idx < self.honest_nodes {
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

        for i in 1..self.validators.len() {
            let voter = self.validators[i];
            if i <= self.honest_nodes {
                self.schedule_prevote_and_precommit(voter, round, proposal_commitment, true, false);
            } else {
                // Faulty node behavior
                self.generate_faulty_traffic(i, round);
            }
        }
    }

    /// Generates traffic for a faulty node at a given round.
    fn generate_faulty_traffic(&mut self, node_idx: usize, round: Round) {
        let node_id = self.validators[node_idx];
        let proposal_commitment = Some(proposal_commitment_for_round(round));

        match self.get_fault_type(node_idx, round) {
            FaultType::Offline => {
                // Send no messages
            }
            FaultType::NilVoter => {
                // Send votes with None commitment (voting for nil)
                self.schedule_prevote_and_precommit(node_id, round, None, false, false);
            }
            FaultType::Equivocator => {
                // Send two conflicting votes (different commitments) for the same round
                // This violates the protocol by sending multiple votes with different commitments
                // First vote with valid commitment should count, second should not count.
                self.schedule_prevote_and_precommit(
                    node_id,
                    round,
                    proposal_commitment,
                    true,
                    true,
                );
            }
            FaultType::UnauthorizedProposer => {
                // Send a proposal even when not the leader
                self.schedule(
                    1,
                    InputEvent::Proposal(ProposalInit {
                        height: HEIGHT_0,
                        round,
                        proposer: node_id,
                        valid_round: None,
                    }),
                );
            }
            FaultType::NonValidator => {
                // Send votes with a voter ID that is outside the validator set
                let non_validator_id = ValidatorId::from(u64::try_from(TOTAL_NODES).unwrap());
                self.schedule_prevote_and_precommit(
                    non_validator_id,
                    round,
                    proposal_commitment,
                    false,
                    false,
                );
            }
        }
    }

    fn check_and_generate_next_round(&mut self, request_round: Round) {
        if self.current_max_round < request_round {
            self.current_max_round = request_round;
            self.generate_round_traffic(request_round);
        }
    }

    /// Runs the simulation until a decision is reached or the deadline is exceeded.
    ///
    /// Returns `Some(Decision)` if consensus is reached, `None` if the deadline
    /// is reached without a decision.
    fn run(&mut self, deadline_ticks: u64) -> Option<Decision> {
        let validators = self.validators.clone();
        let seed = self.seed;
        let leader_fn = move |r: Round| {
            let idx = Self::get_leader_index(seed, r);
            validators[idx]
        };
        // Start the single height consensus
        match self.shc.start(&leader_fn) {
            ShcReturn::Decision(d) => return Some(d),
            ShcReturn::Requests(reqs) => self.handle_requests(reqs),
        }

        // Main event loop
        while let Some(timed_event) = self.pending_events.pop() {
            if timed_event.tick > deadline_ticks {
                break;
            }

            self.current_tick = timed_event.tick;
            self.processed_history.push(timed_event.event.clone());

            // Process the event
            let res = match timed_event.event {
                InputEvent::Vote(v, _should_count) => self.shc.handle_vote(&leader_fn, v),
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
    /// Also tracks BroadcastVote requests to know what the node actually voted for.
    fn handle_requests(&mut self, reqs: VecDeque<SMRequest>) {
        for req in reqs {
            match req {
                SMRequest::StartValidateProposal(init) => {
                    let delay = self.rng.gen_range(15..30);
                    let proposal_commitment = Some(proposal_commitment_for_round(init.round));
                    let result = StateMachineEvent::FinishedValidation(
                        proposal_commitment,
                        init.round,
                        None,
                    );
                    self.schedule(delay, InputEvent::Internal(result));
                }
                SMRequest::StartBuildProposal(round) => {
                    self.check_and_generate_next_round(round);
                    let delay = self.rng.gen_range(15..30);
                    let proposal_commitment = Some(proposal_commitment_for_round(round));
                    let result = StateMachineEvent::FinishedBuilding(proposal_commitment, round);
                    self.schedule(delay, InputEvent::Internal(result));
                }
                SMRequest::ScheduleTimeout(step, round) => {
                    let (delay, event) = match step {
                        Step::Propose => {
                            self.check_and_generate_next_round(round);
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
                _ => {
                    // Ignore other request types
                }
            }
        }
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
            InputEvent::Vote(v, should_count) => {
                if !*should_count {
                    continue;
                }
                if v.vote_type == VoteType::Precommit {
                    if let Some(proposal_commitment) = v.proposal_commitment {
                        let entry = stats.entry(v.round).or_insert_with(|| RoundStats {
                            expected_commitment: proposal_commitment_for_round(v.round),
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
                        expected_commitment: proposal_commitment_for_round(*r),
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

    for r in 0..=sim.current_max_round {
        if let Some(s) = stats.get(&r) {
            // Check what the node actually voted for in this round
            // If the node voted precommit for the valid commitment, count it
            let expected_commitment = proposal_commitment_for_round(r);
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

            // 4. Verify that decision has the same precommits as history for the decided round
            let history_precommits: HashSet<_> = sim
                .processed_history
                .iter()
                .filter_map(|e| {
                    if let InputEvent::Vote(v, should_count) = e {
                        if *should_count
                            && v.vote_type == VoteType::Precommit
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

            // Decision should contain all history precommits, plus possibly the self vote
            assert!(
                history_precommits.is_subset(&decision_precommits),
                "Decision precommits don't contain all history precommits. Decision: {:?}, \
                 History: {:?}",
                actual,
                sim.processed_history
            );

            // Decision should have at most one extra vote (the self vote)
            let extra_votes = decision_precommits.difference(&history_precommits).count();
            assert!(
                extra_votes <= 1,
                "Decision has {} extra precommits, expected at most 1 (self vote). Decision: \
                 {:?}, History: {:?}",
                extra_votes,
                actual,
                sim.processed_history
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

#[test_case(1.0, 100; "keep_all_all_honest")]
#[test_case(0.7, 100; "keep_70%_all_honest")]
#[test_case(1.0, 67; "keep_all_67_honest")]
#[test_case(0.7, 67; "keep_70%_67_honest")]
#[test_case(1.0, 80; "keep_all_80_honest")]
#[test_case(0.9, 80; "keep_90%_80_honest")]
fn test_consensus_simulation(keep_ratio: f64, honest_nodes: usize) {
    let seed = rand::thread_rng().gen();
    println!(
        "Running consensus simulation with total nodes {TOTAL_NODES}, keep ratio {keep_ratio}, \
         honest nodes {honest_nodes} and seed: {seed}"
    );

    let mut sim = DiscreteEventSimulation::new(TOTAL_NODES, honest_nodes, seed, keep_ratio);

    sim.generate_round_traffic(0);

    let result = sim.run(DEADLINE_TICKS);

    verify_result(&sim, result.as_ref());
}
