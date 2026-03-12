use std::collections::VecDeque;

use apollo_protobuf::consensus::{Vote, VoteType, DEFAULT_VALIDATOR_ID};
use apollo_staking::committee_provider::CommitteeError;
use lazy_static::lazy_static;
use starknet_api::block::BlockNumber;
use starknet_api::crypto::utils::RawSignature;
use starknet_types_core::felt::Felt;
use test_case::test_case;

use super::Round;
use crate::state_machine::{SMRequest, StateMachine, StateMachineEvent, Step};
use crate::test_utils::test_committee;
use crate::types::{ProposalCommitment, ValidatorId};
use crate::votes_threshold::QuorumType;

lazy_static! {
    static ref PROPOSER_ID: ValidatorId = DEFAULT_VALIDATOR_ID.into();
    static ref VALIDATOR_ID: ValidatorId = (DEFAULT_VALIDATOR_ID + 1).into();
    static ref VALIDATOR_ID_2: ValidatorId = (DEFAULT_VALIDATOR_ID + 2).into();
    static ref VALIDATOR_ID_3: ValidatorId = (DEFAULT_VALIDATOR_ID + 3).into();
}

const PROPOSAL_ID: Option<ProposalCommitment> = Some(ProposalCommitment(Felt::ONE));
const ROUND: Round = 0;
const HEIGHT: BlockNumber = BlockNumber(0);

/// Actual proposer: returns the leader address (no failure in tests).
type ActualProposerFn = fn(Round) -> ValidatorId;
/// Virtual proposer: can fail (e.g. committee lookup).
type VirtualProposerFn = fn(Round) -> Result<ValidatorId, CommitteeError>;

fn mk_vote(
    vote_type: VoteType,
    round: Round,
    proposal_id: Option<ProposalCommitment>,
    voter: ValidatorId,
) -> Vote {
    Vote {
        vote_type,
        height: HEIGHT,
        round,
        proposal_commitment: proposal_id,
        voter,
        signature: RawSignature::default(),
    }
}

#[track_caller]
fn assert_decision_reached(wrapper: &mut TestWrapper, expected_block: Option<ProposalCommitment>) {
    match wrapper.next_request().unwrap() {
        SMRequest::DecisionReached(dec) => {
            assert_eq!(dec.block, expected_block.unwrap());
            assert!(!dec.precommits.is_empty(), "Decision should have precommits");
        }
        req => panic!("Expected DecisionReached, got {:?}", req),
    }
    assert!(wrapper.next_request().is_none());
}

#[track_caller]
fn assert_start_build_proposal(wrapper: &mut TestWrapper, round: Round) {
    assert_schedule_timeout(wrapper, Step::Propose, round);
    match wrapper.next_request() {
        Some(SMRequest::StartBuildProposal(r)) if r == round => {}
        other => panic!("expected StartBuildProposal({}), got {:?}", round, other),
    }
}

#[track_caller]
fn assert_schedule_timeout(wrapper: &mut TestWrapper, step: Step, round: Round) {
    match wrapper.next_request() {
        Some(SMRequest::ScheduleTimeout(s, r)) if s == step && r == round => {}
        other => panic!("expected ScheduleTimeout({:?}, {}), got {:?}", step, round, other),
    }
}

#[track_caller]
fn assert_broadcast_vote(
    wrapper: &mut TestWrapper,
    vote_type: VoteType,
    round: Round,
    proposal_id: Option<ProposalCommitment>,
    voter: ValidatorId,
) {
    let expected = mk_vote(vote_type, round, proposal_id, voter);
    match wrapper.next_request() {
        Some(SMRequest::BroadcastVote(v)) if v == expected => {}
        other => panic!("expected BroadcastVote({:?}), got {:?}", expected, other),
    }
}

#[track_caller]
fn assert_broadcast_prevote(
    wrapper: &mut TestWrapper,
    round: Round,
    proposal_id: Option<ProposalCommitment>,
    voter: ValidatorId,
) {
    assert_broadcast_vote(wrapper, VoteType::Prevote, round, proposal_id, voter);
}

#[track_caller]
fn assert_broadcast_precommit(
    wrapper: &mut TestWrapper,
    round: Round,
    proposal_id: Option<ProposalCommitment>,
    voter: ValidatorId,
) {
    assert_broadcast_vote(wrapper, VoteType::Precommit, round, proposal_id, voter);
}

#[track_caller]
fn assert_no_more_requests(wrapper: &mut TestWrapper) {
    assert!(wrapper.next_request().is_none(), "expected no more requests, got some");
}

/// After receiving the proposal we get BroadcastPrevote, ScheduleTimeout(Prevote), then with
/// prevote quorum we broadcast precommit for `proposal_id`.
#[track_caller]
fn assert_prevote_quorum_then_precommit(
    wrapper: &mut TestWrapper,
    round: Round,
    proposal_id: Option<ProposalCommitment>,
    self_id: ValidatorId,
) {
    assert_broadcast_prevote(wrapper, round, PROPOSAL_ID, self_id);
    assert_schedule_timeout(wrapper, Step::Prevote, round);
    assert_broadcast_precommit(wrapper, round, proposal_id, self_id);
}

#[track_caller]
fn assert_proposer_nil_prevote_quorum_then_precommit_nil(wrapper: &mut TestWrapper, round: Round) {
    assert_prevote_quorum_then_precommit(wrapper, round, None, *PROPOSER_ID);
}

struct TestWrapper {
    state_machine: StateMachine,
    requests: VecDeque<SMRequest>,
    peer_voters: Vec<ValidatorId>,
    next_peer_idx: usize,
}

impl TestWrapper {
    pub fn new(
        id: ValidatorId,
        total_weight: u64,
        proposer: ActualProposerFn,
        virtual_proposer: VirtualProposerFn,
        is_observer: bool,
        quorum_type: QuorumType,
    ) -> Self {
        let validators = vec![*PROPOSER_ID, *VALIDATOR_ID, *VALIDATOR_ID_2, *VALIDATOR_ID_3];
        let mut peer_voters = validators.iter().filter(|v| **v != id).copied().collect::<Vec<_>>();
        // Ensure deterministic order.
        peer_voters.sort();
        let committee = test_committee(validators, Box::new(proposer), Box::new(virtual_proposer));
        Self {
            state_machine: StateMachine::new(
                HEIGHT,
                id,
                total_weight,
                is_observer,
                quorum_type,
                committee,
                true,
            ),
            requests: VecDeque::new(),
            peer_voters,
            next_peer_idx: 0,
        }
    }

    fn next_peer(&mut self) -> ValidatorId {
        let voter = self.peer_voters[self.next_peer_idx % self.peer_voters.len()];
        self.next_peer_idx += 1;
        voter
    }

    pub fn next_request(&mut self) -> Option<SMRequest> {
        self.requests.pop_front()
    }

    pub fn start(&mut self) {
        self.requests.append(&mut self.state_machine.start())
    }

    pub fn send_finished_building(
        &mut self,
        proposal_id: Option<ProposalCommitment>,
        round: Round,
    ) {
        self.send_event(StateMachineEvent::FinishedBuilding(proposal_id, round))
    }

    pub fn send_finished_validation(
        &mut self,
        proposal_id: Option<ProposalCommitment>,
        round: Round,
    ) {
        self.send_event(StateMachineEvent::FinishedValidation(proposal_id, round, None))
    }

    pub fn send_prevote(&mut self, proposal_id: Option<ProposalCommitment>, round: Round) {
        let voter = self.next_peer();
        self.send_event(StateMachineEvent::Prevote(mk_vote(
            VoteType::Prevote,
            round,
            proposal_id,
            voter,
        )))
    }

    pub fn send_prevote_from(
        &mut self,
        proposal_id: Option<ProposalCommitment>,
        round: Round,
        voter: ValidatorId,
    ) {
        self.send_event(StateMachineEvent::Prevote(mk_vote(
            VoteType::Prevote,
            round,
            proposal_id,
            voter,
        )))
    }

    pub fn send_precommit(&mut self, proposal_id: Option<ProposalCommitment>, round: Round) {
        let voter = self.next_peer();
        self.send_event(StateMachineEvent::Precommit(mk_vote(
            VoteType::Precommit,
            round,
            proposal_id,
            voter,
        )))
    }

    pub fn send_precommit_from(
        &mut self,
        proposal_id: Option<ProposalCommitment>,
        round: Round,
        voter: ValidatorId,
    ) {
        self.send_event(StateMachineEvent::Precommit(mk_vote(
            VoteType::Precommit,
            round,
            proposal_id,
            voter,
        )))
    }

    pub fn send_timeout_propose(&mut self, round: Round) {
        self.send_event(StateMachineEvent::TimeoutPropose(round))
    }

    pub fn send_timeout_prevote(&mut self, round: Round) {
        self.send_event(StateMachineEvent::TimeoutPrevote(round))
    }

    pub fn send_timeout_precommit(&mut self, round: Round) {
        self.send_event(StateMachineEvent::TimeoutPrecommit(round))
    }

    fn send_event(&mut self, event: StateMachineEvent) {
        self.requests.append(&mut self.state_machine.handle_event(event));
    }
}

#[track_caller]
fn advance_proposer_after_start(wrapper: &mut TestWrapper) {
    wrapper.start();
    assert_start_build_proposal(wrapper, ROUND);
    assert_no_more_requests(wrapper);
}

#[track_caller]
fn advance_proposer_to_prevote_broadcast(wrapper: &mut TestWrapper) {
    advance_proposer_after_start(wrapper);
    wrapper.send_finished_building(PROPOSAL_ID, ROUND);
    assert_broadcast_prevote(wrapper, ROUND, PROPOSAL_ID, *PROPOSER_ID);
    assert_no_more_requests(wrapper);
}

#[track_caller]
fn advance_validator_after_start(wrapper: &mut TestWrapper) {
    wrapper.start();
    assert_schedule_timeout(wrapper, Step::Propose, ROUND);
    assert_no_more_requests(wrapper);
}

#[track_caller]
fn advance_validator_to_prevote_broadcast(wrapper: &mut TestWrapper) {
    advance_validator_after_start(wrapper);
    wrapper.send_finished_validation(PROPOSAL_ID, ROUND);
    assert_broadcast_prevote(wrapper, ROUND, PROPOSAL_ID, *VALIDATOR_ID);
    assert_no_more_requests(wrapper);
}

#[track_caller]
fn advance_to_prevote_quorum_and_precommit(wrapper: &mut TestWrapper, self_id: ValidatorId) {
    wrapper.send_prevote(PROPOSAL_ID, ROUND);
    wrapper.send_prevote(PROPOSAL_ID, ROUND);
    assert_schedule_timeout(wrapper, Step::Prevote, ROUND);
    assert_broadcast_precommit(wrapper, ROUND, PROPOSAL_ID, self_id);
    assert_no_more_requests(wrapper);
}

#[track_caller]
fn advance_to_precommit_quorum(wrapper: &mut TestWrapper) {
    wrapper.send_precommit(PROPOSAL_ID, ROUND);
    wrapper.send_precommit(PROPOSAL_ID, ROUND);
    assert_schedule_timeout(wrapper, Step::Precommit, ROUND);
}

#[track_caller]
fn advance_to_precommit_quorum_then_maybe_timeout(
    wrapper: &mut TestWrapper,
    proposal_id: Option<ProposalCommitment>,
    peer_precommit_count: u32,
    timeout: bool,
) {
    for _ in 0..peer_precommit_count {
        wrapper.send_precommit(proposal_id, ROUND);
    }
    assert_schedule_timeout(wrapper, Step::Precommit, ROUND);
    if timeout {
        wrapper.send_timeout_precommit(ROUND);
    }
}

#[test_case(true; "proposer")]
#[test_case(false; "validator")]
fn events_arrive_in_ideal_order(is_proposer: bool) {
    let id = if is_proposer { *PROPOSER_ID } else { *VALIDATOR_ID };
    let mut wrapper = TestWrapper::new(
        id,
        4,
        |_: Round| *PROPOSER_ID,
        |_: Round| Ok(*PROPOSER_ID),
        false,
        QuorumType::Byzantine,
    );

    if is_proposer {
        advance_proposer_to_prevote_broadcast(&mut wrapper);
    } else {
        advance_validator_to_prevote_broadcast(&mut wrapper);
    }
    advance_to_prevote_quorum_and_precommit(&mut wrapper, id);
    advance_to_precommit_quorum(&mut wrapper);
    assert_decision_reached(&mut wrapper, PROPOSAL_ID);
}

#[test]
fn validator_receives_votes_first() {
    let mut wrapper = TestWrapper::new(
        *VALIDATOR_ID,
        4,
        |_: Round| *PROPOSER_ID,
        |_: Round| Ok(*PROPOSER_ID),
        false,
        QuorumType::Byzantine,
    );

    advance_validator_after_start(&mut wrapper);

    // Receives votes from all the other nodes first (more than minimum for a quorum).
    for _ in 0..3 {
        wrapper.send_prevote(PROPOSAL_ID, ROUND);
        wrapper.send_precommit(PROPOSAL_ID, ROUND);
    }

    // The Node got a Precommit quorum. TimeoutPrevote is only initiated once the SM reaches the
    // prevote step.
    assert_eq!(wrapper.next_request().unwrap(), SMRequest::ScheduleTimeout(Step::Precommit, ROUND));
    assert!(wrapper.next_request().is_none());

    // Finally the proposal arrives.
    wrapper.send_finished_validation(PROPOSAL_ID, ROUND);
    assert_prevote_quorum_then_precommit(&mut wrapper, ROUND, PROPOSAL_ID, *VALIDATOR_ID);
    assert_decision_reached(&mut wrapper, PROPOSAL_ID);
}

#[test_case(PROPOSAL_ID ; "valid_proposal")]
#[test_case(None ; "invalid_proposal")]
fn buffer_events_during_get_proposal(vote: Option<ProposalCommitment>) {
    let mut wrapper = TestWrapper::new(
        *PROPOSER_ID,
        4,
        |_: Round| *PROPOSER_ID,
        |_: Round| Ok(*PROPOSER_ID),
        false,
        QuorumType::Byzantine,
    );

    advance_proposer_after_start(&mut wrapper);
    for _ in 0..3 {
        wrapper.send_prevote(vote, ROUND);
    }
    assert_no_more_requests(&mut wrapper);

    // Node finishes building the proposal.
    wrapper.send_finished_building(PROPOSAL_ID, ROUND);
    assert_prevote_quorum_then_precommit(&mut wrapper, ROUND, vote, *PROPOSER_ID);
    assert_no_more_requests(&mut wrapper);
}

#[test]
fn only_send_precommit_with_prevote_quorum_and_proposal() {
    let mut wrapper = TestWrapper::new(
        *VALIDATOR_ID,
        4,
        |_: Round| *PROPOSER_ID,
        |_: Round| Ok(*PROPOSER_ID),
        false,
        QuorumType::Byzantine,
    );

    advance_validator_after_start(&mut wrapper);

    // Receives votes from all the other nodes first (more than minimum for a quorum).
    for _ in 0..3 {
        wrapper.send_prevote(PROPOSAL_ID, ROUND);
    }
    assert_no_more_requests(&mut wrapper);

    // Finally the proposal arrives.
    wrapper.send_finished_validation(PROPOSAL_ID, ROUND);
    assert_prevote_quorum_then_precommit(&mut wrapper, ROUND, PROPOSAL_ID, *VALIDATOR_ID);
    assert_no_more_requests(&mut wrapper);
}

#[test]
fn only_decide_with_prcommit_quorum_and_proposal() {
    let mut wrapper = TestWrapper::new(
        *VALIDATOR_ID,
        4,
        |_: Round| *PROPOSER_ID,
        |_: Round| Ok(*PROPOSER_ID),
        false,
        QuorumType::Byzantine,
    );

    advance_validator_after_start(&mut wrapper);

    // Receives votes from all the other nodes first (more than minimum for a quorum).
    for _ in 0..3 {
        wrapper.send_prevote(PROPOSAL_ID, ROUND);
    }
    for _ in 0..2 {
        wrapper.send_precommit(PROPOSAL_ID, ROUND);
    }
    assert_no_more_requests(&mut wrapper);

    // Finally the proposal arrives.
    wrapper.send_finished_validation(PROPOSAL_ID, ROUND);
    assert_prevote_quorum_then_precommit(&mut wrapper, ROUND, PROPOSAL_ID, *VALIDATOR_ID);
    assert_schedule_timeout(&mut wrapper, Step::Precommit, ROUND);
    assert_decision_reached(&mut wrapper, PROPOSAL_ID);
}

#[test]
fn advance_to_the_next_round() {
    let mut wrapper = TestWrapper::new(
        *VALIDATOR_ID,
        4,
        |_: Round| *PROPOSER_ID,
        |_: Round| Ok(*PROPOSER_ID),
        false,
        QuorumType::Byzantine,
    );

    advance_validator_to_prevote_broadcast(&mut wrapper);
    for _ in 0..2 {
        wrapper.send_precommit(None, ROUND);
    }
    assert_no_more_requests(&mut wrapper);

    wrapper.send_finished_validation(PROPOSAL_ID, ROUND + 1);
    assert!(wrapper.next_request().is_none());

    wrapper.send_precommit(None, ROUND);
    assert_eq!(wrapper.next_request().unwrap(), SMRequest::ScheduleTimeout(Step::Precommit, ROUND));
    wrapper.send_timeout_precommit(ROUND);
    // The Node sends Prevote after advancing to the next round.
    assert_eq!(
        wrapper.next_request().unwrap(),
        SMRequest::ScheduleTimeout(Step::Propose, ROUND + 1)
    );
    assert_eq!(
        wrapper.next_request().unwrap(),
        SMRequest::BroadcastVote(mk_vote(VoteType::Prevote, ROUND + 1, PROPOSAL_ID, *VALIDATOR_ID))
    );
}

#[test]
fn prevote_when_receiving_proposal_in_current_round() {
    let mut wrapper = TestWrapper::new(
        *VALIDATOR_ID,
        4,
        |_: Round| *PROPOSER_ID,
        |_: Round| Ok(*PROPOSER_ID),
        false,
        QuorumType::Byzantine,
    );

    advance_validator_after_start(&mut wrapper);
    advance_to_precommit_quorum_then_maybe_timeout(&mut wrapper, None, 3, true);

    // The node starts the next round, shouldn't prevote when receiving a proposal for the
    // previous round.
    assert_eq!(
        wrapper.next_request().unwrap(),
        SMRequest::ScheduleTimeout(Step::Propose, ROUND + 1)
    );
    wrapper.send_finished_validation(PROPOSAL_ID, ROUND);
    assert!(wrapper.next_request().is_none());
    // The node should prevote when receiving a proposal for the current round.
    wrapper.send_finished_validation(PROPOSAL_ID, ROUND + 1);
    assert_eq!(
        wrapper.next_request().unwrap(),
        SMRequest::BroadcastVote(mk_vote(VoteType::Prevote, ROUND + 1, PROPOSAL_ID, *VALIDATOR_ID))
    );
}

#[test_case(true ; "send_proposal")]
#[test_case(false ; "send_timeout_propose")]
fn mixed_quorum(send_proposal: bool) {
    let mut wrapper = TestWrapper::new(
        *VALIDATOR_ID,
        4,
        |_: Round| *PROPOSER_ID,
        |_: Round| Ok(*PROPOSER_ID),
        false,
        QuorumType::Byzantine,
    );

    if send_proposal {
        advance_validator_to_prevote_broadcast(&mut wrapper);
    } else {
        advance_validator_after_start(&mut wrapper);
        wrapper.send_timeout_propose(ROUND);
        assert_broadcast_prevote(&mut wrapper, ROUND, None, *VALIDATOR_ID);
    }
    wrapper.send_prevote(PROPOSAL_ID, ROUND);
    wrapper.send_prevote(None, ROUND);
    // The Node got a Prevote quorum.
    assert_eq!(wrapper.next_request().unwrap(), SMRequest::ScheduleTimeout(Step::Prevote, ROUND));
    wrapper.send_timeout_prevote(ROUND);
    assert_eq!(
        wrapper.next_request().unwrap(),
        SMRequest::BroadcastVote(mk_vote(VoteType::Precommit, ROUND, None, *VALIDATOR_ID))
    );
    advance_to_precommit_quorum_then_maybe_timeout(&mut wrapper, PROPOSAL_ID, 2, true);
    assert_schedule_timeout(&mut wrapper, Step::Propose, ROUND + 1);
}

#[test]
fn dont_handle_enqueued_while_awaiting_get_proposal() {
    let mut wrapper = TestWrapper::new(
        *PROPOSER_ID,
        4,
        |_: Round| *PROPOSER_ID,
        |_: Round| Ok(*PROPOSER_ID),
        false,
        QuorumType::Byzantine,
    );

    advance_proposer_after_start(&mut wrapper);

    // We simulate that this node is always the proposer, but it lagged, so the peers kept voting
    // NIL and progressing rounds.
    for _ in 0..3 {
        wrapper.send_prevote(None, ROUND);
        wrapper.send_precommit(None, ROUND);
    }
    for _ in 0..3 {
        wrapper.send_prevote(None, ROUND + 1);
        wrapper.send_precommit(None, ROUND + 1);
    }
    // It now receives the proposal.
    wrapper.send_finished_building(PROPOSAL_ID, ROUND);
    assert_proposer_nil_prevote_quorum_then_precommit_nil(&mut wrapper, ROUND);
    assert_schedule_timeout(&mut wrapper, Step::Precommit, ROUND);

    // Timeout and advance on to the next round.
    wrapper.send_timeout_precommit(ROUND);
    assert_start_build_proposal(&mut wrapper, ROUND + 1);
    assert_no_more_requests(&mut wrapper);

    // The other votes are only handled after the next GetProposal is received.
    wrapper.send_finished_building(PROPOSAL_ID, ROUND + 1);
    assert_proposer_nil_prevote_quorum_then_precommit_nil(&mut wrapper, ROUND + 1);
    assert_schedule_timeout(&mut wrapper, Step::Precommit, ROUND + 1);
}

#[test]
fn return_proposal_if_locked_value_is_set() {
    let mut wrapper = TestWrapper::new(
        *PROPOSER_ID,
        4,
        |_: Round| *PROPOSER_ID,
        |_: Round| Ok(*PROPOSER_ID),
        false,
        QuorumType::Byzantine,
    );

    advance_proposer_to_prevote_broadcast(&mut wrapper);
    advance_to_prevote_quorum_and_precommit(&mut wrapper, *PROPOSER_ID);
    advance_to_precommit_quorum_then_maybe_timeout(&mut wrapper, None, 2, true);

    // no need to GetProposal since we already have a locked value.
    assert_eq!(
        wrapper.next_request().unwrap(),
        SMRequest::Repropose(
            PROPOSAL_ID.unwrap(),
            apollo_protobuf::consensus::BuildParam {
                height: HEIGHT,
                round: ROUND + 1,
                proposer: *PROPOSER_ID,
                valid_round: Some(ROUND),
            }
        )
    );
    assert_eq!(
        wrapper.next_request().unwrap(),
        SMRequest::BroadcastVote(mk_vote(VoteType::Prevote, ROUND + 1, PROPOSAL_ID, *PROPOSER_ID))
    );
}

#[test]
fn observer_node_reaches_decision() {
    let id = *VALIDATOR_ID;
    let mut wrapper = TestWrapper::new(
        id,
        4,
        |_: Round| *PROPOSER_ID,
        |_: Round| Ok(*PROPOSER_ID),
        true,
        QuorumType::Byzantine,
    );

    advance_validator_after_start(&mut wrapper);
    wrapper.send_finished_validation(PROPOSAL_ID, ROUND);
    // The observer node does not respond to the proposal by sending votes.
    assert_no_more_requests(&mut wrapper);

    advance_to_precommit_quorum_then_maybe_timeout(&mut wrapper, PROPOSAL_ID, 3, false);
    // Once a quorum of precommits is observed, the node should generate a decision event.
    assert_decision_reached(&mut wrapper, PROPOSAL_ID);
}

#[test_case(QuorumType::Byzantine; "byzantine")]
#[test_case(QuorumType::Honest; "honest")]
fn number_of_required_votes(quorum_type: QuorumType) {
    let mut wrapper = TestWrapper::new(
        *VALIDATOR_ID,
        3,
        |_: Round| *PROPOSER_ID,
        |_: Round| Ok(*PROPOSER_ID),
        false,
        quorum_type,
    );

    advance_validator_to_prevote_broadcast(&mut wrapper);

    // Another node sends a Prevote (vote 2).
    wrapper.send_prevote(PROPOSAL_ID, ROUND);

    // Byzantine quorum requires 3 votes, so we need one more vote.
    if quorum_type == QuorumType::Byzantine {
        // Not enough votes for a quorum yet.
        assert_no_more_requests(&mut wrapper);

        // Another node sends a Prevote (vote 3).
        wrapper.send_prevote(PROPOSAL_ID, ROUND);
    }
    // In honest case, the second vote is enough for a quorum.

    assert_schedule_timeout(&mut wrapper, Step::Prevote, ROUND);
    assert_broadcast_precommit(&mut wrapper, ROUND, PROPOSAL_ID, *VALIDATOR_ID);
    assert_no_more_requests(&mut wrapper);

    // The virtual proposer sends a Precommit (vote 2).
    wrapper.send_precommit_from(PROPOSAL_ID, ROUND, *PROPOSER_ID);

    // Byzantine quorum requires 3 votes, so we need one more vote.
    if quorum_type == QuorumType::Byzantine {
        // Not enough votes for a quorum yet.
        assert_no_more_requests(&mut wrapper);

        // Another node sends a Precommit (vote 3).
        wrapper.send_precommit_from(PROPOSAL_ID, ROUND, *VALIDATOR_ID_2);
    }
    // In honest case, the second vote is enough for a quorum.

    // The Node got a Precommit quorum.
    assert_schedule_timeout(&mut wrapper, Step::Precommit, ROUND);
    assert_decision_reached(&mut wrapper, PROPOSAL_ID);
}

#[test]
fn observer_does_not_record_self_votes() {
    // Set up as an observer.
    let id = *VALIDATOR_ID;
    let mut wrapper = TestWrapper::new(
        id,
        4,
        |_: Round| *PROPOSER_ID,
        |_: Round| Ok(*PROPOSER_ID),
        true,
        QuorumType::Byzantine,
    );

    advance_validator_after_start(&mut wrapper);
    wrapper.send_finished_validation(PROPOSAL_ID, ROUND);

    // Reach mixed prevote quorum with peer votes only (self not counted).
    for _ in 0..2 {
        wrapper.send_prevote(PROPOSAL_ID, ROUND);
    }
    // No quorum yet, we didn't vote.
    assert!(wrapper.next_request().is_none());
    wrapper.send_prevote(PROPOSAL_ID, ROUND);
    assert_eq!(wrapper.next_request().unwrap(), SMRequest::ScheduleTimeout(Step::Prevote, ROUND));

    // Timeout prevote triggers self precommit(nil) path, which observers must not record/broadcast.
    wrapper.send_timeout_prevote(ROUND);
    assert!(wrapper.next_request().is_none());
    assert_eq!(wrapper.state_machine.last_self_precommit(), None);

    // Reach mixed precommit quorum with peer votes only and ensure timeout is scheduled.
    for _ in 0..2 {
        wrapper.send_precommit(PROPOSAL_ID, ROUND);
    }
    // No quorum yet, we didn't vote.
    assert!(wrapper.next_request().is_none());
    wrapper.send_precommit(PROPOSAL_ID, ROUND);
    assert_eq!(wrapper.next_request().unwrap(), SMRequest::ScheduleTimeout(Step::Precommit, ROUND));
}

#[test]
fn quorums_require_virtual_proposer_in_favor_for_value() {
    // Virtual proposer (VALIDATOR_ID_3) must be one of the voters in favor for precommit quorum for
    // a value to reach decision.
    let mut wrapper = TestWrapper::new(
        *VALIDATOR_ID,
        4,
        |_: Round| *PROPOSER_ID,
        |_: Round| Ok(*VALIDATOR_ID_3),
        false,
        QuorumType::Byzantine,
    );

    advance_validator_to_prevote_broadcast(&mut wrapper);

    // Reach prevote quorum without the virtual proposer's prevote (self + 2 peers).
    wrapper.send_prevote_from(PROPOSAL_ID, ROUND, *PROPOSER_ID); // peer 1
    assert_no_more_requests(&mut wrapper);
    wrapper.send_prevote_from(PROPOSAL_ID, ROUND, *VALIDATOR_ID_2); // peer 2

    assert_schedule_timeout(&mut wrapper, Step::Prevote, ROUND);
    assert_broadcast_precommit(&mut wrapper, ROUND, PROPOSAL_ID, *VALIDATOR_ID);
    assert_no_more_requests(&mut wrapper);

    // Reach precommit quorum without the virtual proposer's precommit (self + 2 peers).
    wrapper.send_precommit_from(PROPOSAL_ID, ROUND, *PROPOSER_ID); // peer 1
    assert_no_more_requests(&mut wrapper);
    wrapper.send_precommit_from(PROPOSAL_ID, ROUND, *VALIDATOR_ID_2); // peer 2

    // Mixed precommit quorum still schedules timeout, but we must NOT decide yet.
    assert_schedule_timeout(&mut wrapper, Step::Precommit, ROUND);
    assert_no_more_requests(&mut wrapper);

    // Now the virtual proposer precommits for the value -> we can decide.
    wrapper.send_precommit_from(PROPOSAL_ID, ROUND, *VALIDATOR_ID_3); // peer 3 (virtual proposer)
    assert_decision_reached(&mut wrapper, PROPOSAL_ID);
}

#[test]
fn advance_to_round_when_proposer_function_fails() {
    // Test that when virtual proposer function fails during round advancement,
    // we act as a validator and schedule a timeout to prevent deadlock,
    // and can still progress via receiving 2/3 precommits.
    fn actual_proposer_fn(_round: Round) -> ValidatorId {
        *PROPOSER_ID
    }

    fn virtual_proposer_fn(round: Round) -> Result<ValidatorId, CommitteeError> {
        if round == 1 { Err(CommitteeError::EmptyCommittee) } else { Ok(*PROPOSER_ID) }
    }

    let round_1: Round = 1;
    let mut wrapper = TestWrapper::new(
        *VALIDATOR_ID,
        4,
        actual_proposer_fn,
        virtual_proposer_fn,
        false, // not observer
        QuorumType::Byzantine,
    );

    advance_validator_after_start(&mut wrapper);

    // Receive 2 precommits for round 1 (1/3 threshold with total_weight=4 needs >4/3, so 2 votes)
    // This should trigger advancement to round 1
    wrapper.send_precommit_from(PROPOSAL_ID, round_1, *PROPOSER_ID);
    assert!(wrapper.next_request().is_none());
    wrapper.send_precommit_from(PROPOSAL_ID, round_1, *VALIDATOR_ID_2);
    // When advancing to round 1, proposer lookup fails, so we act as validator
    // and schedule timeout to prevent deadlock. Round should be updated.
    assert_eq!(wrapper.next_request().unwrap(), SMRequest::ScheduleTimeout(Step::Propose, round_1));
    assert_eq!(wrapper.state_machine.round(), round_1);
    assert!(wrapper.next_request().is_none());

    // Now receive 2/3 precommits for round 1 (3 precommits with total_weight=4)
    // This should schedule timeout for precommit
    wrapper.send_precommit_from(PROPOSAL_ID, round_1, *VALIDATOR_ID_3);
    // We should schedule timeout for precommit when we get 2/3 precommits
    assert_eq!(
        wrapper.next_request().unwrap(),
        SMRequest::ScheduleTimeout(Step::Precommit, round_1)
    );
    assert!(wrapper.next_request().is_none());
}

#[test]
fn timeout_prevote_ignored_when_wrong_step() {
    fn actual_proposer_fn(_round: Round) -> ValidatorId {
        *PROPOSER_ID
    }
    fn virtual_proposer_fn(_round: Round) -> Result<ValidatorId, CommitteeError> {
        Ok(*PROPOSER_ID)
    }
    let mut wrapper = TestWrapper::new(
        *VALIDATOR_ID,
        4,
        actual_proposer_fn,
        virtual_proposer_fn,
        false,
        QuorumType::Byzantine,
    );
    advance_validator_to_prevote_broadcast(&mut wrapper);
    advance_to_prevote_quorum_and_precommit(&mut wrapper, *VALIDATOR_ID);
    // Now in Precommit step. TimeoutPrevote for current round should be ignored.
    wrapper.send_timeout_prevote(ROUND);
    assert!(wrapper.next_request().is_none());
}

#[test]
fn no_repropose_when_virtual_proposer_fails_for_new_round() {
    fn actual_proposer_fn(_round: Round) -> ValidatorId {
        *PROPOSER_ID
    }
    fn virtual_proposer_fn(round: Round) -> Result<ValidatorId, CommitteeError> {
        if round == ROUND + 1 { Err(CommitteeError::EmptyCommittee) } else { Ok(*PROPOSER_ID) }
    }
    let mut wrapper = TestWrapper::new(
        *PROPOSER_ID,
        4,
        actual_proposer_fn,
        virtual_proposer_fn,
        false,
        QuorumType::Byzantine,
    );
    advance_proposer_to_prevote_broadcast(&mut wrapper);
    advance_to_prevote_quorum_and_precommit(&mut wrapper, *PROPOSER_ID);
    advance_to_precommit_quorum_then_maybe_timeout(&mut wrapper, None, 2, true);
    // Proposer advances to round 1 with locked value but virtual_proposer(1) fails -> no Repropose.
    assert_no_more_requests(&mut wrapper);
    assert_eq!(wrapper.state_machine.round(), ROUND + 1);
}

#[test]
fn prevote_nil_when_new_proposal_differs_from_locked_value() {
    fn actual_proposer_fn(_round: Round) -> ValidatorId {
        *PROPOSER_ID
    }
    fn virtual_proposer_fn(_round: Round) -> Result<ValidatorId, CommitteeError> {
        Ok(*PROPOSER_ID)
    }
    const OTHER_PROPOSAL: Option<ProposalCommitment> = Some(ProposalCommitment(Felt::TWO));
    let round_1: Round = 1;
    let mut wrapper = TestWrapper::new(
        *VALIDATOR_ID,
        4,
        actual_proposer_fn,
        virtual_proposer_fn,
        false,
        QuorumType::Byzantine,
    );
    advance_validator_to_prevote_broadcast(&mut wrapper);
    advance_to_prevote_quorum_and_precommit(&mut wrapper, *VALIDATOR_ID);
    advance_to_precommit_quorum_then_maybe_timeout(&mut wrapper, None, 2, true);
    assert_schedule_timeout(&mut wrapper, Step::Propose, round_1);
    // We have locked value PROPOSAL_ID. Receive a different proposal for round 1.
    wrapper.send_finished_validation(OTHER_PROPOSAL, round_1);
    // Should prevote nil (None) because proposal differs from locked value.
    assert_broadcast_prevote(&mut wrapper, round_1, None, *VALIDATOR_ID);
}

#[test]
fn timeout_propose_unblocks_proposer_while_awaiting_finished_building() {
    // Proposer in awaiting_finished_building receives TimeoutPropose; must advance to Prevote
    // and broadcast nil, same as validator timeout. Build never returned.
    let mut wrapper = TestWrapper::new(
        *PROPOSER_ID,
        4,
        |_: Round| *PROPOSER_ID,
        |_: Round| Ok(*PROPOSER_ID),
        false,
        QuorumType::Byzantine,
    );
    wrapper.start();
    assert_start_build_proposal(&mut wrapper, ROUND);
    assert_no_more_requests(&mut wrapper);

    // Simulate build timeout: TimeoutPropose fires while still awaiting FinishedBuilding.
    wrapper.send_timeout_propose(ROUND);
    // Proposer advances to Prevote with nil, like a validator.
    assert_broadcast_prevote(&mut wrapper, ROUND, None, *PROPOSER_ID);
    assert_no_more_requests(&mut wrapper);

    // awaiting_finished_building should be changed to false, so we can handle the received
    // prevotes.
    wrapper.send_prevote(None, ROUND);
    wrapper.send_prevote(None, ROUND);
    // Nil prevote quorum triggers ScheduleTimeout(Prevote) and nil precommit broadcast.
    assert_schedule_timeout(&mut wrapper, Step::Prevote, ROUND);
    assert_broadcast_precommit(&mut wrapper, ROUND, None, *PROPOSER_ID);
    assert_no_more_requests(&mut wrapper);
}

#[test]
fn timeout_propose_then_build_finishes() {
    // TimeoutPropose fires first (build appeared to hang), then FinishedBuilding arrives late.
    // Both must be processed; the late FinishedBuilding inserts the proposal without panic.
    let mut wrapper = TestWrapper::new(
        *PROPOSER_ID,
        4,
        |_: Round| *PROPOSER_ID,
        |_: Round| Ok(*PROPOSER_ID),
        false,
        QuorumType::Byzantine,
    );
    wrapper.start();
    assert_start_build_proposal(&mut wrapper, ROUND);
    assert_no_more_requests(&mut wrapper);

    // TimeoutPropose fires first (build timed out at consensus level).
    wrapper.send_timeout_propose(ROUND);
    assert_broadcast_prevote(&mut wrapper, ROUND, None, *PROPOSER_ID);
    assert_no_more_requests(&mut wrapper);

    // Build finishes late; must be processed without panic. Proposal is ignored.
    wrapper.send_finished_building(PROPOSAL_ID, ROUND);
    assert_no_more_requests(&mut wrapper);
}
