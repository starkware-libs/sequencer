//! Discrete event simulation test for consensus protocol.
//!
//! This test uses a discrete event simulation approach with a timeline-based
//! event queue.
//!
//! Messages are scheduled with random delays to simulate network jitter.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};
use std::ops::Range;

use apollo_consensus_config::config::TimeoutsConfig;
use apollo_protobuf::consensus::{ProposalInit, Vote, VoteType};
use lazy_static::lazy_static;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use starknet_api::block::BlockNumber;
use starknet_api::crypto::utils::RawSignature;
use starknet_types_core::felt::Felt;
use strum::{EnumIter, IntoEnumIterator};
use test_case::test_case;

use crate::single_height_consensus::SingleHeightConsensus;
use crate::state_machine::{SMRequest, StateMachineEvent, Step};
use crate::test_utils::mock_committee_virtual_equal_to_actual;
use crate::types::{Decision, ProposalCommitment, Round, ValidatorId};
use crate::votes_threshold::QuorumType;

const HEIGHT_0: BlockNumber = BlockNumber(0);
const NODE_0_LEADER_PROBABILITY: f64 = 0.1;
const NODE_UNDER_TEST: usize = 0;

// Timing configuration (all values in ticks)
//
// NOTE: These timing ranges are NOT strict protocol requirements. Messages can arrive
// at any time and rounds can overlap freely. The timing model exists only to create
// realistic clustering of messages - most votes for a given round happen near each other.
//
// The ONLY hard constraints that matter for correctness are:
// 1. Each node sends precommit AFTER prevote (for the same round)
// 2. Honest nodes send votes AFTER seeing the proposal (when NODE_0 is the proposer)
//
// Everything else (round boundaries, overlap, etc.) is just for simulation realism.
const ROUND_DURATION: u64 = 100; // Each round spans 100 ticks
const ROUND_OVERLAP_PERCENT: u64 = 20; // Overlap between rounds as percentage of ROUND_DURATION
const ROUND_OVERLAP: u64 = ROUND_DURATION * ROUND_OVERLAP_PERCENT / 100;

// Network delays for messages (min..max ticks)
const PROPOSAL_ARRIVAL_DELAY_RANGE: Range<u64> = 2..20;
const PREVOTE_ARRIVAL_DELAY_RANGE: Range<u64> = 20..50;
const PRECOMMIT_AFTER_PREVOTE_DELAY_RANGE: Range<u64> = 5..20;

// Processing delays
const VALIDATION_DELAY_RANGE: Range<u64> = 15..30; // Time to validate a proposal
const BUILD_PROPOSAL_DELAY_RANGE: Range<u64> = 15..30; // Time to build a proposal

// Timeout delays
const TIMEOUT_PROPOSE_DELAY_RANGE: Range<u64> = 15..30; // Timeout for propose step
const TIMEOUT_PREVOTE_DELAY_RANGE: Range<u64> = 5..10; // Timeout for prevote step
const TIMEOUT_PRECOMMIT_DELAY_RANGE: Range<u64> = 5..10; // Timeout for precommit step

lazy_static! {
    static ref NODE_0: ValidatorId = ValidatorId::from(u64::try_from(NODE_UNDER_TEST).unwrap());
}

/// Types of faulty behavior that nodes can exhibit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter)]
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

/// Generates a proposal commitment for the given round.
/// If `is_fake` is true, generates a fake commitment.
fn proposal_commitment_for_round(round: Round, is_fake: bool) -> ProposalCommitment {
    let offset = if is_fake { 9999 } else { 0 };
    ProposalCommitment(Felt::from(u64::from(round) + offset))
}

/// Probabilistically selects a leader index for the given round.
/// Node 0 (the one under test) has probability NODE_0_LEADER_PROBABILITY of being selected.
/// Other nodes share the remaining probability (1 - NODE_0_LEADER_PROBABILITY) uniformly.
/// The selection is deterministic per round - the same round will always return the same
/// leader index.
fn get_leader_index(seed: u64, total_nodes: usize, round: Round) -> usize {
    let round_u64 = u64::from(round);
    let round_seed = seed.wrapping_mul(31).wrapping_add(round_u64);
    let mut round_rng = StdRng::seed_from_u64(round_seed);

    let random_value: f64 = round_rng.gen();

    if random_value < NODE_0_LEADER_PROBABILITY {
        NODE_UNDER_TEST
    } else {
        round_rng.gen_range(1..total_nodes)
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
    /// Total number of nodes in the network.
    total_nodes: usize,
    /// Number of honest nodes (the rest are faulty).
    honest_nodes: usize,
    /// Quorum threshold for reaching consensus (2/3 + 1 of total nodes).
    quorum_threshold: usize,
    /// The single height consensus instance.
    shc: SingleHeightConsensus,
    /// All validators in the network.
    validators: Vec<ValidatorId>,
    /// Priority queue of pending events that have yet to be processed (min-heap by tick).
    pending_events: BinaryHeap<TimedEvent>,
    /// Current simulation tick.
    current_tick: u64,
    /// History of all processed events.
    processed_history: Vec<InputEvent>,
    /// Tracks what the node actually voted for in each round.
    node_votes: HashMap<Round, Vote>,
    /// Number of rounds to pre-generate.
    num_rounds: usize,
    /// Tracks which rounds NODE_0 is the proposer.
    node_0_proposer_rounds: HashSet<Round>,
    /// The keep ratio for the network (probability that messages are not dropped).
    keep_ratio: f64,
    /// Tracks precommits and proposal status per round and commitment.
    /// Key: (round, proposal_commitment), Value: (precommits set, finished_proposal flag)
    round_stats: HashMap<(Round, ProposalCommitment), (HashSet<Vote>, bool)>,
    /// Tracks which voters have already voted for each round (to detect conflicts/duplicates).
    voter_first_vote: HashSet<(Round, ValidatorId)>,
}

impl DiscreteEventSimulation {
    fn new(
        total_nodes: usize,
        honest_nodes: usize,
        seed: u64,
        num_rounds: usize,
        keep_ratio: f64,
    ) -> Self {
        assert!(honest_nodes <= total_nodes, "honest_nodes must be <= total_nodes");
        assert!((0.0..=1.0).contains(&keep_ratio), "keep_ratio must be between 0.0 and 1.0");
        let rng = StdRng::seed_from_u64(seed);
        let validators: Vec<ValidatorId> =
            (0..total_nodes).map(|i| ValidatorId::from(u64::try_from(i).unwrap())).collect();

        let committee = mock_committee_virtual_equal_to_actual(
            validators.clone(),
            Box::new({
                let validators = validators.clone();
                move |round| {
                    let idx = get_leader_index(seed, total_nodes, round);
                    validators[idx]
                }
            }),
        );

        let shc = SingleHeightConsensus::new(
            HEIGHT_0,
            false,
            *NODE_0,
            validators.clone(),
            QuorumType::Byzantine,
            TimeoutsConfig::default(),
            committee,
            true,
        );

        let quorum_threshold = (2 * total_nodes / 3) + 1;

        Self {
            rng,
            seed,
            total_nodes,
            honest_nodes,
            quorum_threshold,
            shc,
            validators,
            pending_events: BinaryHeap::new(),
            current_tick: 0,
            processed_history: Vec::new(),
            node_votes: HashMap::new(),
            num_rounds,
            node_0_proposer_rounds: HashSet::new(),
            keep_ratio,
            round_stats: HashMap::new(),
            voter_first_vote: HashSet::new(),
        }
    }

    /// Determines the fault type for a faulty node at a given round.
    /// Uses deterministic randomness based on node index and round.
    fn get_fault_type(&mut self, node_idx: usize, round: Round) -> FaultType {
        let node_idx_u64 = u64::try_from(node_idx).unwrap();
        let round_u64 = u64::from(round);
        let seed = self
            .seed
            .wrapping_mul(31)
            .wrapping_add(node_idx_u64)
            .wrapping_mul(31)
            .wrapping_add(round_u64);
        let mut fault_rng = StdRng::seed_from_u64(seed);

        // Randomly select a fault type
        let fault_types: Vec<FaultType> = FaultType::iter().collect();
        *fault_types.choose(&mut fault_rng).unwrap()
    }

    /// Schedules an event to occur at the specified absolute tick.
    /// Internal events are always scheduled.
    /// Other events are scheduled with probability keep_ratio.
    fn schedule_at_tick(&mut self, tick: u64, event: InputEvent) {
        let should_enqueue =
            matches!(event, InputEvent::Internal(_)) || self.rng.gen_bool(self.keep_ratio);
        if should_enqueue {
            self.pending_events.push(TimedEvent { tick, event });
        }
    }

    /// Schedules both prevote and precommit for a voter in a round.
    /// Generates random delays internally to simulate network jitter.
    /// `base_tick` is the base tick from which delays are calculated.
    fn schedule_prevote_and_precommit(
        &mut self,
        voter: ValidatorId,
        round: Round,
        commitment: Option<ProposalCommitment>,
        round_start_tick: u64,
    ) {
        let round_end_tick = round_start_tick + ROUND_DURATION;
        let prevote_tick = (round_start_tick + self.rng.gen_range(PREVOTE_ARRIVAL_DELAY_RANGE))
            .min(round_end_tick - 1);
        let precommit_tick = (prevote_tick
            + self.rng.gen_range(PRECOMMIT_AFTER_PREVOTE_DELAY_RANGE))
        .min(round_end_tick);

        self.schedule_at_tick(
            prevote_tick,
            InputEvent::Vote(Vote {
                vote_type: VoteType::Prevote,
                height: HEIGHT_0,
                round,
                proposal_commitment: commitment,
                voter,
                signature: RawSignature::default(),
            }),
        );
        self.schedule_at_tick(
            precommit_tick,
            InputEvent::Vote(Vote {
                vote_type: VoteType::Precommit,
                height: HEIGHT_0,
                round,
                proposal_commitment: commitment,
                voter,
                signature: RawSignature::default(),
            }),
        );
    }

    /// Pre-generates all events for all requested rounds.
    ///
    /// Each round gets its own time range with minimal overlap.
    /// For rounds where NODE_0 is the proposer, peer votes are scheduled after
    /// the build finish event (which will be determined dynamically during simulation).
    fn pre_generate_all_rounds(&mut self) {
        for round_idx in 0..self.num_rounds {
            let round = Round::from(u32::try_from(round_idx).unwrap());
            let leader_idx = get_leader_index(self.seed, self.total_nodes, round);
            let leader_id = self.validators[leader_idx];
            // Track rounds where NODE_0 is the proposer.
            // We will schedule peer votes for these rounds after the build finish event.
            if leader_idx == NODE_UNDER_TEST {
                self.node_0_proposer_rounds.insert(round);
                continue;
            }

            // Determine time range for this round
            let round_start_tick =
                u64::try_from(round_idx).unwrap() * (ROUND_DURATION - ROUND_OVERLAP);

            // 1. Proposal from leader (if not NODE_0 and leader is honest)
            if leader_idx < self.honest_nodes {
                let proposal_tick = (round_start_tick
                    + self.rng.gen_range(PROPOSAL_ARRIVAL_DELAY_RANGE))
                .min(round_start_tick + ROUND_DURATION);
                self.schedule_at_tick(
                    proposal_tick,
                    InputEvent::Proposal(ProposalInit {
                        height: HEIGHT_0,
                        round,
                        proposer: leader_id,
                        valid_round: None,
                        ..Default::default()
                    }),
                );
            }
            // 2. Votes from other honest validators
            self.schedule_peer_votes(round, round_start_tick);
        }
    }

    /// Schedules peer votes for a round. Votes are scheduled within the round time range.
    ///
    /// Note: The timing constraints here are not strict - in reality, votes can arrive at any
    /// time and may overlap with other rounds. The timing ranges just create realistic clustering.
    /// The only real constraint enforced is: precommit_tick > prevote_tick (same voter).
    fn schedule_peer_votes(&mut self, round: Round, round_start_tick: u64) {
        let proposal_commitment = Some(proposal_commitment_for_round(round, false));
        // Skip index 0 (self) - our votes are handled by the state machine
        for i in 1..self.validators.len() {
            let voter = self.validators[i];
            if i < self.honest_nodes {
                // Honest node behavior: send normal votes
                self.schedule_prevote_and_precommit(
                    voter,
                    round,
                    proposal_commitment,
                    round_start_tick,
                );
            } else {
                // Faulty node behavior - schedule within round time range
                self.generate_faulty_traffic_at_ticks(i, round, round_start_tick);
            }
        }
    }

    /// Generates traffic for a faulty node at a given round.
    fn generate_faulty_traffic_at_ticks(
        &mut self,
        node_idx: usize,
        round: Round,
        round_start_tick: u64,
    ) {
        let node_id = self.validators[node_idx];
        let proposal_commitment = Some(proposal_commitment_for_round(round, false));

        match self.get_fault_type(node_idx, round) {
            FaultType::Offline => {
                // Send no messages
            }
            FaultType::NilVoter => {
                self.schedule_prevote_and_precommit(node_id, round, None, round_start_tick);
            }
            FaultType::Equivocator => {
                let fake_commitment = Some(proposal_commitment_for_round(round, true));
                // Send two conflicting votes for the same round.
                // The actual time scheduling is random, so either ordering is possible (both are
                // faulty).
                self.schedule_prevote_and_precommit(
                    node_id,
                    round,
                    proposal_commitment,
                    round_start_tick,
                );
                self.schedule_prevote_and_precommit(
                    node_id,
                    round,
                    fake_commitment,
                    round_start_tick,
                );
            }
            FaultType::UnauthorizedProposer => {
                // Send a proposal even when not the leader
                self.schedule_at_tick(
                    round_start_tick + 1,
                    InputEvent::Proposal(ProposalInit {
                        height: HEIGHT_0,
                        round,
                        proposer: node_id,
                        valid_round: None,
                        ..Default::default()
                    }),
                );
            }
            FaultType::NonValidator => {
                // Send votes with a voter ID that is outside the validator set
                let non_validator_id = ValidatorId::from(u64::try_from(self.total_nodes).unwrap());
                self.schedule_prevote_and_precommit(
                    non_validator_id,
                    round,
                    proposal_commitment,
                    round_start_tick,
                );
            }
        }
    }

    /// Tracks a precommit vote for the given round and commitment.
    /// Only tracks votes from validators and only the first vote from each voter per round
    fn track_precommit(&mut self, vote: &Vote) {
        if vote.vote_type != VoteType::Precommit {
            return;
        }
        if !self.validators.contains(&vote.voter) {
            return;
        }
        if let Some(commitment) = vote.proposal_commitment {
            let voter_key = (vote.round, vote.voter);
            if !self.voter_first_vote.insert(voter_key) {
                // Already voted for this round, ignore subsequent votes
                return;
            }

            let key = (vote.round, commitment);
            let (precommits, _) =
                self.round_stats.entry(key).or_insert_with(|| (HashSet::new(), false));
            precommits.insert(vote.clone());
        }
    }

    /// Tracks that a proposal finished (validation or building) for the given round and commitment.
    fn track_finished_proposal(&mut self, round: Round, commitment: Option<ProposalCommitment>) {
        if let Some(commitment) = commitment {
            let key = (round, commitment);
            let entry = self.round_stats.entry(key).or_insert_with(|| (HashSet::new(), false));
            entry.1 = true;
        }
    }

    /// Runs the simulation until a decision is reached or the deadline is exceeded.
    ///
    /// Returns `Some(Decision)` if consensus is reached, `None` if the deadline
    /// is reached without a decision.
    fn run(&mut self, deadline_ticks: u64) -> Option<Decision> {
        // Pre-generate all rounds events
        self.pre_generate_all_rounds();
        // Create two separate closures with the same logic (for proposer and virtual_proposer)
        // Start the single height consensus
        let requests = self.shc.start();
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

            // Track and process the event
            let requests = match timed_event.event {
                InputEvent::Vote(v) => {
                    self.track_precommit(&v);
                    self.shc.handle_vote(v)
                }
                InputEvent::Proposal(p) => self.shc.handle_proposal(p),
                InputEvent::Internal(StateMachineEvent::FinishedValidation(
                    commitment,
                    round,
                    _,
                )) => {
                    self.track_finished_proposal(round, commitment);
                    self.shc.handle_event(StateMachineEvent::FinishedValidation(
                        commitment, round, None,
                    ))
                }
                InputEvent::Internal(StateMachineEvent::FinishedBuilding(commitment, round)) => {
                    self.track_finished_proposal(round, commitment);
                    self.shc.handle_event(StateMachineEvent::FinishedBuilding(commitment, round))
                }
                InputEvent::Internal(e) => self.shc.handle_event(e),
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
    /// Also tracks BroadcastVote requests to know what the node actually voted for.
    fn handle_requests(&mut self, reqs: VecDeque<SMRequest>) -> Option<Decision> {
        for req in reqs {
            match req {
                SMRequest::StartValidateProposal(init) => {
                    let delay = self.rng.gen_range(VALIDATION_DELAY_RANGE);
                    let validate_finish_tick = self.current_tick + delay;
                    let proposal_commitment =
                        Some(proposal_commitment_for_round(init.round, false));
                    let result = StateMachineEvent::FinishedValidation(
                        proposal_commitment,
                        init.round,
                        None,
                    );
                    self.schedule_at_tick(validate_finish_tick, InputEvent::Internal(result));
                }
                SMRequest::StartBuildProposal(round) => {
                    let delay = self.rng.gen_range(BUILD_PROPOSAL_DELAY_RANGE);
                    let build_finish_tick = self.current_tick + delay;
                    let proposal_commitment = Some(proposal_commitment_for_round(round, false));
                    let result = StateMachineEvent::FinishedBuilding(proposal_commitment, round);
                    self.schedule_at_tick(build_finish_tick, InputEvent::Internal(result));

                    // Schedule peer votes after build finish
                    assert!(self.node_0_proposer_rounds.contains(&round));
                    self.schedule_peer_votes(round, build_finish_tick);
                }
                SMRequest::ScheduleTimeout(step, round) => {
                    let (delay, event) = match step {
                        Step::Propose => (
                            self.rng.gen_range(TIMEOUT_PROPOSE_DELAY_RANGE),
                            StateMachineEvent::TimeoutPropose(round),
                        ),
                        Step::Prevote => (
                            self.rng.gen_range(TIMEOUT_PREVOTE_DELAY_RANGE),
                            StateMachineEvent::TimeoutPrevote(round),
                        ),
                        Step::Precommit => (
                            self.rng.gen_range(TIMEOUT_PRECOMMIT_DELAY_RANGE),
                            StateMachineEvent::TimeoutPrecommit(round),
                        ),
                    };
                    let timeout_tick = self.current_tick + delay;
                    self.schedule_at_tick(timeout_tick, InputEvent::Internal(event));
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
        }
        None
    }
}

fn verify_result(sim: &DiscreteEventSimulation, result: Option<&Decision>) {
    // Determine expected decision based on tracked round stats
    // A decision should be reached when:
    // 1. Proposal finished (validation or building)
    // 2. At least THRESHOLD precommits from validators for that round+commitment
    // 3. The virtual proposer for that round precommitted in favor of the block
    let expected_decision =
        sim.round_stats.iter().find_map(|((r, commitment), (precommits, proposal_ready))| {
            if !*proposal_ready {
                return None;
            }

            // Check if we have enough precommits (including possibly self vote)
            let peer_precommits = precommits.len();
            let self_vote = sim
                .node_votes
                .get(r)
                .filter(|v| {
                    v.vote_type == VoteType::Precommit && v.proposal_commitment == Some(*commitment)
                })
                .iter()
                .count();
            let total_precommits = peer_precommits + self_vote;
            // Match the state machine's `virtual_proposer_in_favor` gating for decision.
            // In this simulation the virtual leader function is the same as the leader function.
            let virtual_proposer = {
                let idx = get_leader_index(sim.seed, sim.total_nodes, *r);
                sim.validators[idx]
            };
            let virtual_proposer_precommitted_in_favor = if virtual_proposer == *NODE_0 {
                self_vote == 1
            } else {
                precommits.iter().any(|v| v.voter == virtual_proposer)
            };

            if total_precommits >= sim.quorum_threshold && virtual_proposer_precommitted_in_favor {
                Some((*r, *commitment, precommits.clone()))
            } else {
                None
            }
        });

    let expected_str = expected_decision
        .as_ref()
        .map(|(r, c, _)| format!("Some(({r:?}, {c:?}))"))
        .unwrap_or_else(|| "None".to_string());

    // 3. Compare with Actual Result
    match (result, expected_decision) {
        (Some(actual), Some((expected_round, expected_commitment, tracked_precommits))) => {
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

            // 4. Verify that decision precommits contain the tracked precommits
            let decision_precommits: HashSet<_> = actual.precommits.iter().cloned().collect();

            // Check that the difference (decision precommits not in history) is only the node's
            // vote
            let diff: HashSet<_> = decision_precommits.difference(&tracked_precommits).collect();
            // Get the node's own vote (if it voted for this round/proposal)
            let expected_diff: HashSet<_> = sim
                .node_votes
                .get(&decided_round)
                .filter(|v| {
                    v.vote_type == VoteType::Precommit
                        && v.proposal_commitment == Some(decided_block)
                })
                .iter()
                .cloned()
                .collect();
            assert_eq!(
                diff, expected_diff,
                "Decision has precommits not in history that don't match node vote. Diff: {:?}, \
                 History: {:?}, Decision: {:?}",
                diff, tracked_precommits, actual.precommits
            );

            // Verify quorum threshold is met
            assert!(
                actual.precommits.len() >= sim.quorum_threshold,
                "Insufficient precommits in decision: {}/{}. Decision: {:?}, History: {:?}",
                actual.precommits.len(),
                sim.quorum_threshold,
                actual,
                sim.processed_history
            );
        }
        (None, None) => {
            // SUCCESS: No decision reached. History confirms conditions were never met.
        }
        _ => {
            panic!(
                "FAILURE: returned {result:?}, expected {expected_str}. History: {:?}",
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
    let num_rounds = 5; // Number of rounds to pre-generate
    let total_nodes = 100;
    println!(
        "Running consensus simulation with total nodes {total_nodes}, {num_rounds} rounds, keep \
         ratio {keep_ratio}, honest nodes {honest_nodes} and seed: {seed}"
    );

    let mut sim =
        DiscreteEventSimulation::new(total_nodes, honest_nodes, seed, num_rounds, keep_ratio);

    let deadline_ticks = u64::try_from(num_rounds).unwrap() * ROUND_DURATION;
    let result = sim.run(deadline_ticks);

    verify_result(&sim, result.as_ref());
}
