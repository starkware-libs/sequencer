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
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use starknet_api::block::BlockNumber;
use starknet_types_core::felt::Felt;
use strum::{EnumIter, IntoEnumIterator};
use test_case::test_case;

use crate::single_height_consensus::SingleHeightConsensus;
use crate::state_machine::{SMRequest, StateMachineEvent, Step};
use crate::types::{Decision, ProposalCommitment, Round, ValidatorId};
use crate::votes_threshold::QuorumType;

const HEIGHT_0: BlockNumber = BlockNumber(0);
const TOTAL_NODES: usize = 100;
const THRESHOLD: usize = (2 * TOTAL_NODES / 3) + 1;
const DEADLINE_TICKS: u64 = 200;
const NODE_0_LEADER_PROBABILITY: f64 = 0.1;
const NODE_UNDER_TEST: usize = 0;

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
    /// Sends two conflicting votes for the same round (real commitment first, then fake).
    EquivocatorRealFirst,
    /// Sends two conflicting votes for the same round (fake commitment first, then real).
    EquivocatorFakeFirst,
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
    current_max_round: Option<Round>,
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
    /// The keep ratio for the network (probability that messages are not dropped).
    keep_ratio: f64,
    /// Tracks precommits and proposal status per round and commitment.
    /// Key: (round, proposal_commitment), Value: (precommits set, finished_proposal flag)
    round_stats: HashMap<(Round, ProposalCommitment), (HashSet<Vote>, bool)>,
    /// Tracks which voters have already voted for each round (to detect conflicts/duplicates).
    voter_first_vote: HashSet<(Round, ValidatorId)>,
}

impl DiscreteEventSimulation {
    fn new(total_nodes: usize, honest_nodes: usize, seed: u64, keep_ratio: f64) -> Self {
        assert!(honest_nodes <= total_nodes, "honest_nodes must be <= total_nodes");
        assert!((0.0..=1.0).contains(&keep_ratio), "keep_ratio must be between 0.0 and 1.0");
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
            honest_nodes,
            current_max_round: None,
            pending_events: BinaryHeap::new(),
            current_tick: 0,
            processed_history: Vec::new(),
            build_finish_info: HashMap::new(),
            node_votes: HashMap::new(),
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
            NODE_UNDER_TEST
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
    fn schedule_prevote_and_precommit(
        &mut self,
        voter: ValidatorId,
        round: Round,
        commitment: Option<ProposalCommitment>,
    ) {
        let base = self
            .build_finish_info
            .get(&round)
            .map(
                |(delay, proposal_commitment)| {
                    if &commitment == proposal_commitment { *delay } else { 0 }
                },
            )
            .unwrap_or(0);
        let prevote_delay = self.rng.gen_range(2..20) + base;
        let precommit_delta = self.rng.gen_range(5..20);

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

    /// Generates traffic for a specific round with a given keep ratio.
    ///
    /// - Proposer sends: Proposal -> Prevote -> Precommit (in order)
    /// - Other validators send: Prevote -> Precommit (in order)
    ///
    /// Messages are scheduled with random delays to simulate network jitter,
    /// but each node's messages maintain correct ordering.
    ///
    /// Honest nodes behave correctly, while faulty nodes exhibit various fault behaviors.
    fn generate_round_traffic(&mut self, round: Round) {
        let leader_idx = Self::get_leader_index(self.seed, round);
        let leader_id = self.validators[leader_idx];
        let proposal_commitment = Some(proposal_commitment_for_round(round, false));

        // Handle leader proposal (only if leader is honest and not node 0)
        if leader_idx != NODE_UNDER_TEST && leader_idx <= self.honest_nodes {
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

        // Process all validators (excluding node 0 which is the one under test)
        for i in 1..self.validators.len() {
            let voter = self.validators[i];

            if i <= self.honest_nodes {
                // Honest node behavior: send normal votes
                self.schedule_prevote_and_precommit(voter, round, proposal_commitment);
            } else {
                // Faulty node behavior
                self.generate_faulty_traffic(i, round);
            }
        }
    }

    /// Generates traffic for a faulty node at a given round.
    fn generate_faulty_traffic(&mut self, node_idx: usize, round: Round) {
        let node_id = self.validators[node_idx];
        let proposal_commitment = Some(proposal_commitment_for_round(round, false));

        match self.get_fault_type(node_idx, round) {
            FaultType::Offline => {
                // Send no messages
            }
            FaultType::NilVoter => {
                self.schedule_prevote_and_precommit(node_id, round, None);
            }
            FaultType::EquivocatorRealFirst => {
                self.schedule_prevote_and_precommit(node_id, round, proposal_commitment);
                let fake_commitment = Some(proposal_commitment_for_round(round, true));
                self.schedule_prevote_and_precommit(node_id, round, fake_commitment);
            }
            FaultType::EquivocatorFakeFirst => {
                let fake_commitment = Some(proposal_commitment_for_round(round, true));
                self.schedule_prevote_and_precommit(node_id, round, fake_commitment);
                self.schedule_prevote_and_precommit(node_id, round, proposal_commitment);
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
                self.schedule_prevote_and_precommit(non_validator_id, round, proposal_commitment);
            }
        }
    }

    fn check_and_generate_next_round(&mut self) {
        if self.current_max_round.is_none()
            || self.current_max_round.unwrap() < self.shc.current_round()
        {
            self.current_max_round = Some(self.shc.current_round());
            self.generate_round_traffic(self.shc.current_round());
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
        let validators = self.validators.clone();
        let seed = self.seed;
        let leader_fn = move |r: Round| {
            let idx = Self::get_leader_index(seed, r);
            validators[idx]
        };

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

            // Track and process the event
            let requests = match timed_event.event {
                InputEvent::Vote(v) => {
                    self.track_precommit(&v);
                    self.shc.handle_vote(&leader_fn, v)
                }
                InputEvent::Proposal(p) => self.shc.handle_proposal(&leader_fn, p),
                InputEvent::Internal(StateMachineEvent::FinishedValidation(
                    commitment,
                    round,
                    _,
                )) => {
                    self.track_finished_proposal(round, commitment);
                    self.shc.handle_event(
                        &leader_fn,
                        StateMachineEvent::FinishedValidation(commitment, round, None),
                    )
                }
                InputEvent::Internal(StateMachineEvent::FinishedBuilding(commitment, round)) => {
                    self.track_finished_proposal(round, commitment);
                    self.shc.handle_event(
                        &leader_fn,
                        StateMachineEvent::FinishedBuilding(commitment, round),
                    )
                }
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
    /// Also tracks BroadcastVote requests to know what the node actually voted for.
    fn handle_requests(&mut self, reqs: VecDeque<SMRequest>) -> Option<Decision> {
        for req in reqs {
            match req {
                SMRequest::StartValidateProposal(init) => {
                    let delay = self.rng.gen_range(15..30);
                    let proposal_commitment =
                        Some(proposal_commitment_for_round(init.round, false));
                    let result = StateMachineEvent::FinishedValidation(
                        proposal_commitment,
                        init.round,
                        None,
                    );
                    self.schedule(delay, InputEvent::Internal(result));
                }
                SMRequest::StartBuildProposal(round) => {
                    let delay = self.rng.gen_range(15..30);
                    let proposal_commitment = Some(proposal_commitment_for_round(round, false));
                    let result = StateMachineEvent::FinishedBuilding(proposal_commitment, round);
                    self.build_finish_info.insert(round, (delay, proposal_commitment));
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
    // Determine expected decision based on tracked round stats
    // A decision should be reached when:
    // 1. Proposal finished (validation or building)
    // 2. At least THRESHOLD precommits from validators for that round+commitment
    let expected_decision =
        sim.round_stats.iter().find_map(|((r, commitment), (precommits, proposal_ready))| {
            if !*proposal_ready {
                return None;
            }

            // Check if we have enough precommits (including possibly self vote)
            let peer_precommits = precommits.len();
            let self_vote = sim.node_votes.get(r).map_or(0, |v| {
                if v.vote_type == VoteType::Precommit && v.proposal_commitment == Some(*commitment)
                {
                    1
                } else {
                    0
                }
            });
            let total_precommits = peer_precommits + self_vote;

            if total_precommits >= THRESHOLD {
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
            let diff: HashSet<_> =
                decision_precommits.difference(&tracked_precommits).cloned().collect();
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
                diff, tracked_precommits, actual.precommits
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
    println!(
        "Running consensus simulation with total nodes {TOTAL_NODES},  keep ratio {keep_ratio}, \
         honest nodes {honest_nodes} and seed: {seed}"
    );

    let mut sim = DiscreteEventSimulation::new(TOTAL_NODES, honest_nodes, seed, keep_ratio);

    let result = sim.run(DEADLINE_TICKS);

    verify_result(&sim, result.as_ref());
}
