use std::collections::VecDeque;

use apollo_protobuf::consensus::DEFAULT_VALIDATOR_ID;
use lazy_static::lazy_static;
use starknet_api::block::BlockHash;
use starknet_types_core::felt::Felt;
use test_case::test_case;

use super::Round;
use crate::state_machine::{StateMachine, StateMachineEvent};
use crate::types::{ProposalCommitment, ValidatorId};
use crate::votes_threshold::QuorumType;

lazy_static! {
    static ref PROPOSER_ID: ValidatorId = DEFAULT_VALIDATOR_ID.into();
    static ref VALIDATOR_ID: ValidatorId = (DEFAULT_VALIDATOR_ID + 1).into();
}

const PROPOSAL_ID: Option<ProposalCommitment> = Some(BlockHash(Felt::ONE));
const ROUND: Round = 0;

struct TestWrapper<LeaderFn: Fn(Round) -> ValidatorId> {
    state_machine: StateMachine,
    leader_fn: LeaderFn,
    events: VecDeque<StateMachineEvent>,
}

impl<LeaderFn: Fn(Round) -> ValidatorId> TestWrapper<LeaderFn> {
    pub fn new(
        id: ValidatorId,
        total_weight: u64,
        leader_fn: LeaderFn,
        is_observer: bool,
        quorum_type: QuorumType,
    ) -> Self {
        Self {
            state_machine: StateMachine::new(id, total_weight, is_observer, quorum_type),
            leader_fn,
            events: VecDeque::new(),
        }
    }

    pub fn next_event(&mut self) -> Option<StateMachineEvent> {
        self.events.pop_front()
    }

    pub fn start(&mut self) {
        self.events.append(&mut self.state_machine.start(&self.leader_fn))
    }

    pub fn send_get_proposal(&mut self, proposal_id: Option<ProposalCommitment>, round: Round) {
        self.send_event(StateMachineEvent::GetProposal(proposal_id, round))
    }

    pub fn send_proposal(&mut self, proposal_id: Option<ProposalCommitment>, round: Round) {
        self.send_event(StateMachineEvent::Proposal(proposal_id, round, None))
    }

    pub fn send_prevote(&mut self, proposal_id: Option<ProposalCommitment>, round: Round) {
        self.send_event(StateMachineEvent::Prevote(proposal_id, round))
    }

    pub fn send_precommit(&mut self, proposal_id: Option<ProposalCommitment>, round: Round) {
        self.send_event(StateMachineEvent::Precommit(proposal_id, round))
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
        self.events.append(&mut self.state_machine.handle_event(event, &self.leader_fn));
    }
}

#[test_case(true; "proposer")]
#[test_case(false; "validator")]
fn events_arrive_in_ideal_order(is_proposer: bool) {
    let id = if is_proposer { *PROPOSER_ID } else { *VALIDATOR_ID };
    let mut wrapper =
        TestWrapper::new(id, 4, |_: Round| *PROPOSER_ID, false, QuorumType::Byzantine);

    wrapper.start();
    if is_proposer {
        assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::GetProposal(None, ROUND));
        wrapper.send_get_proposal(PROPOSAL_ID, ROUND);
        assert_eq!(
            wrapper.next_event().unwrap(),
            StateMachineEvent::Proposal(PROPOSAL_ID, ROUND, None)
        );
    } else {
        // Waiting for the proposal.
        assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPropose(ROUND));
        assert!(wrapper.next_event().is_none());
        wrapper.send_proposal(PROPOSAL_ID, ROUND);
    }
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Prevote(PROPOSAL_ID, ROUND));
    assert!(wrapper.next_event().is_none());

    wrapper.send_prevote(PROPOSAL_ID, ROUND);
    assert!(wrapper.next_event().is_none());

    wrapper.send_prevote(PROPOSAL_ID, ROUND);
    // The Node got a Prevote quorum.
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPrevote(ROUND));
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Precommit(PROPOSAL_ID, ROUND));
    assert!(wrapper.next_event().is_none());

    wrapper.send_precommit(PROPOSAL_ID, ROUND);
    assert!(wrapper.next_event().is_none());

    wrapper.send_precommit(PROPOSAL_ID, ROUND);
    // The Node got a Precommit quorum.
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPrecommit(ROUND));
    assert_eq!(
        wrapper.next_event().unwrap(),
        StateMachineEvent::Decision(PROPOSAL_ID.unwrap(), ROUND)
    );
    assert!(wrapper.next_event().is_none());
}

#[test]
fn validator_receives_votes_first() {
    let mut wrapper =
        TestWrapper::new(*VALIDATOR_ID, 4, |_: Round| *PROPOSER_ID, false, QuorumType::Byzantine);

    wrapper.start();
    // Waiting for the proposal.
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPropose(ROUND));
    assert!(wrapper.next_event().is_none());

    // Receives votes from all the other nodes first (more than minimum for a quorum).
    wrapper.send_prevote(PROPOSAL_ID, ROUND);
    wrapper.send_prevote(PROPOSAL_ID, ROUND);
    wrapper.send_prevote(PROPOSAL_ID, ROUND);
    wrapper.send_precommit(PROPOSAL_ID, ROUND);
    wrapper.send_precommit(PROPOSAL_ID, ROUND);
    wrapper.send_precommit(PROPOSAL_ID, ROUND);
    // The Node got a Precommit quorum. TimeoutPrevote is only initiated once the SM reaches the
    // prevote step.
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPrecommit(ROUND));
    assert!(wrapper.next_event().is_none());

    // Finally the proposal arrives.
    wrapper.send_proposal(PROPOSAL_ID, ROUND);
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Prevote(PROPOSAL_ID, ROUND));
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPrevote(ROUND));
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Precommit(PROPOSAL_ID, ROUND));
    assert_eq!(
        wrapper.next_event().unwrap(),
        StateMachineEvent::Decision(PROPOSAL_ID.unwrap(), ROUND)
    );
    assert!(wrapper.next_event().is_none());
}

#[test_case(PROPOSAL_ID ; "valid_proposal")]
#[test_case(None ; "invalid_proposal")]
fn buffer_events_during_get_proposal(vote: Option<ProposalCommitment>) {
    let mut wrapper =
        TestWrapper::new(*PROPOSER_ID, 4, |_: Round| *PROPOSER_ID, false, QuorumType::Byzantine);

    wrapper.start();
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::GetProposal(None, 0));
    assert!(wrapper.next_event().is_none());

    wrapper.send_prevote(vote, ROUND);
    wrapper.send_prevote(vote, ROUND);
    wrapper.send_prevote(vote, ROUND);
    assert!(wrapper.next_event().is_none());

    // Node finishes building the proposal.
    wrapper.send_get_proposal(PROPOSAL_ID, ROUND);
    assert_eq!(
        wrapper.next_event().unwrap(),
        StateMachineEvent::Proposal(PROPOSAL_ID, ROUND, None)
    );
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Prevote(PROPOSAL_ID, ROUND));
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPrevote(ROUND));
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Precommit(vote, ROUND));
    assert!(wrapper.next_event().is_none());
}

#[test]
fn only_send_precommit_with_prevote_quorum_and_proposal() {
    let mut wrapper =
        TestWrapper::new(*VALIDATOR_ID, 4, |_: Round| *PROPOSER_ID, false, QuorumType::Byzantine);

    wrapper.start();
    // Waiting for the proposal.
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPropose(ROUND));
    assert!(wrapper.next_event().is_none());

    // Receives votes from all the other nodes first (more than minimum for a quorum).
    wrapper.send_prevote(PROPOSAL_ID, ROUND);
    wrapper.send_prevote(PROPOSAL_ID, ROUND);
    wrapper.send_prevote(PROPOSAL_ID, ROUND);
    assert!(wrapper.next_event().is_none());

    // Finally the proposal arrives.
    wrapper.send_proposal(PROPOSAL_ID, ROUND);
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Prevote(PROPOSAL_ID, ROUND));
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPrevote(ROUND));
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Precommit(PROPOSAL_ID, ROUND));
    assert!(wrapper.next_event().is_none());
}

#[test]
fn only_decide_with_prcommit_quorum_and_proposal() {
    let mut wrapper =
        TestWrapper::new(*VALIDATOR_ID, 4, |_: Round| *PROPOSER_ID, false, QuorumType::Byzantine);

    wrapper.start();
    // Waiting for the proposal.
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPropose(ROUND));
    assert!(wrapper.next_event().is_none());

    // Receives votes from all the other nodes first (more than minimum for a quorum).
    wrapper.send_prevote(PROPOSAL_ID, ROUND);
    wrapper.send_prevote(PROPOSAL_ID, ROUND);
    wrapper.send_prevote(PROPOSAL_ID, ROUND);
    wrapper.send_precommit(PROPOSAL_ID, ROUND);
    wrapper.send_precommit(PROPOSAL_ID, ROUND);
    assert!(wrapper.next_event().is_none());

    // Finally the proposal arrives.
    wrapper.send_proposal(PROPOSAL_ID, ROUND);
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Prevote(PROPOSAL_ID, ROUND));
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPrevote(ROUND));
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Precommit(PROPOSAL_ID, ROUND));
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPrecommit(ROUND));
    assert_eq!(
        wrapper.next_event().unwrap(),
        StateMachineEvent::Decision(PROPOSAL_ID.unwrap(), ROUND)
    );
    assert!(wrapper.next_event().is_none());
}

#[test]
fn advance_to_the_next_round() {
    let mut wrapper =
        TestWrapper::new(*VALIDATOR_ID, 4, |_: Round| *PROPOSER_ID, false, QuorumType::Byzantine);

    wrapper.start();
    // Waiting for the proposal.
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPropose(ROUND));
    assert!(wrapper.next_event().is_none());

    wrapper.send_proposal(PROPOSAL_ID, ROUND);
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Prevote(PROPOSAL_ID, ROUND));
    wrapper.send_precommit(None, ROUND);
    wrapper.send_precommit(None, ROUND);
    assert!(wrapper.next_event().is_none());

    wrapper.send_proposal(PROPOSAL_ID, ROUND + 1);
    assert!(wrapper.next_event().is_none());

    wrapper.send_precommit(None, ROUND);
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPrecommit(ROUND));
    wrapper.send_timeout_precommit(ROUND);
    // The Node sends Prevote after advancing to the next round.
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPropose(ROUND + 1));
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Prevote(PROPOSAL_ID, ROUND + 1));
}

#[test]
fn prevote_when_receiving_proposal_in_current_round() {
    let mut wrapper =
        TestWrapper::new(*VALIDATOR_ID, 4, |_: Round| *PROPOSER_ID, false, QuorumType::Byzantine);

    wrapper.start();
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPropose(ROUND));
    assert!(wrapper.next_event().is_none());

    wrapper.send_precommit(None, ROUND);
    wrapper.send_precommit(None, ROUND);
    wrapper.send_precommit(None, ROUND);
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPrecommit(ROUND));
    wrapper.send_timeout_precommit(ROUND);

    // The node starts the next round, shouldn't prevote when receiving a proposal for the
    // previous round.
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPropose(ROUND + 1));
    wrapper.send_proposal(PROPOSAL_ID, ROUND);
    assert!(wrapper.next_event().is_none());
    // The node should prevote when receiving a proposal for the current round.
    wrapper.send_proposal(PROPOSAL_ID, ROUND + 1);
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Prevote(PROPOSAL_ID, ROUND + 1));
}

#[test_case(true ; "send_proposal")]
#[test_case(false ; "send_timeout_propose")]
fn mixed_quorum(send_proposal: bool) {
    let mut wrapper =
        TestWrapper::new(*VALIDATOR_ID, 4, |_: Round| *PROPOSER_ID, false, QuorumType::Byzantine);

    wrapper.start();
    // Waiting for the proposal.
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPropose(ROUND));
    assert!(wrapper.events.is_empty());

    if send_proposal {
        wrapper.send_proposal(PROPOSAL_ID, ROUND);
        assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Prevote(PROPOSAL_ID, ROUND));
    } else {
        wrapper.send_timeout_propose(ROUND);
        assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Prevote(None, ROUND));
    }
    wrapper.send_prevote(PROPOSAL_ID, ROUND);
    wrapper.send_prevote(None, ROUND);
    // The Node got a Prevote quorum.
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPrevote(ROUND));
    wrapper.send_timeout_prevote(ROUND);
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Precommit(None, ROUND));
    wrapper.send_precommit(PROPOSAL_ID, ROUND);
    wrapper.send_precommit(PROPOSAL_ID, ROUND);
    // The Node got a Precommit quorum.
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPrecommit(ROUND));
    wrapper.send_timeout_precommit(ROUND);
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPropose(ROUND + 1));
}

#[test]
fn dont_handle_enqueued_while_awaiting_get_proposal() {
    let mut wrapper =
        TestWrapper::new(*PROPOSER_ID, 4, |_: Round| *PROPOSER_ID, false, QuorumType::Byzantine);

    wrapper.start();
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::GetProposal(None, ROUND));
    assert!(wrapper.next_event().is_none());

    // We simulate that this node is always the proposer, but it lagged, so the peers kept voting
    // NIL and progressing rounds.
    wrapper.send_prevote(None, ROUND);
    wrapper.send_prevote(None, ROUND);
    wrapper.send_prevote(None, ROUND);
    wrapper.send_precommit(None, ROUND);
    wrapper.send_precommit(None, ROUND);
    wrapper.send_precommit(None, ROUND);
    wrapper.send_prevote(None, ROUND + 1);
    wrapper.send_prevote(None, ROUND + 1);
    wrapper.send_prevote(None, ROUND + 1);
    wrapper.send_precommit(None, ROUND + 1);
    wrapper.send_precommit(None, ROUND + 1);
    wrapper.send_precommit(None, ROUND + 1);

    // It now receives the proposal.
    wrapper.send_get_proposal(PROPOSAL_ID, ROUND);
    assert_eq!(
        wrapper.next_event().unwrap(),
        StateMachineEvent::Proposal(PROPOSAL_ID, ROUND, None)
    );
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPrecommit(ROUND));

    // Timeout and advance on to the next round.
    wrapper.send_timeout_precommit(ROUND);
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::GetProposal(None, ROUND + 1));
    assert!(wrapper.next_event().is_none());

    // The other votes are only handled after the next GetProposal is received.
    wrapper.send_get_proposal(PROPOSAL_ID, ROUND + 1);
    assert_eq!(
        wrapper.next_event().unwrap(),
        StateMachineEvent::Proposal(PROPOSAL_ID, ROUND + 1, None)
    );
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPrecommit(ROUND + 1));
}

#[test]
fn return_proposal_if_locked_value_is_set() {
    let mut wrapper =
        TestWrapper::new(*PROPOSER_ID, 4, |_: Round| *PROPOSER_ID, false, QuorumType::Byzantine);

    wrapper.start();
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::GetProposal(None, ROUND));
    assert!(wrapper.next_event().is_none());

    wrapper.send_get_proposal(PROPOSAL_ID, ROUND);
    assert_eq!(
        wrapper.next_event().unwrap(),
        StateMachineEvent::Proposal(PROPOSAL_ID, ROUND, None)
    );
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Prevote(PROPOSAL_ID, ROUND));
    // locked_value is set after receiving a Prevote quorum.
    wrapper.send_prevote(PROPOSAL_ID, ROUND);
    wrapper.send_prevote(PROPOSAL_ID, ROUND);
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPrevote(ROUND));
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Precommit(PROPOSAL_ID, ROUND));

    wrapper.send_precommit(None, ROUND);
    wrapper.send_precommit(None, ROUND);
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPrecommit(ROUND));

    wrapper.send_timeout_precommit(ROUND);

    // no need to GetProposal since we already have a locked value.
    assert_eq!(
        wrapper.next_event().unwrap(),
        StateMachineEvent::Proposal(PROPOSAL_ID, ROUND + 1, Some(ROUND))
    );
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Prevote(PROPOSAL_ID, ROUND + 1));
}

#[test]
fn observer_node_reaches_decision() {
    let id = *VALIDATOR_ID;
    let mut wrapper = TestWrapper::new(id, 4, |_: Round| *PROPOSER_ID, true, QuorumType::Byzantine);

    wrapper.start();

    // Waiting for the proposal.
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPropose(ROUND));
    assert!(wrapper.next_event().is_none());
    wrapper.send_proposal(PROPOSAL_ID, ROUND);
    // The observer node does not respond to the proposal by sending votes.
    assert!(wrapper.next_event().is_none());

    wrapper.send_precommit(PROPOSAL_ID, ROUND);
    wrapper.send_precommit(PROPOSAL_ID, ROUND);
    wrapper.send_precommit(PROPOSAL_ID, ROUND);
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPrecommit(ROUND));
    // Once a quorum of precommits is observed, the node should generate a decision event.
    assert_eq!(
        wrapper.next_event().unwrap(),
        StateMachineEvent::Decision(PROPOSAL_ID.unwrap(), ROUND)
    );
    assert!(wrapper.next_event().is_none());
}

#[test_case(QuorumType::Byzantine; "byzantine")]
#[test_case(QuorumType::Honest; "honest")]
fn number_of_required_votes(quorum_type: QuorumType) {
    let mut wrapper =
        TestWrapper::new(*VALIDATOR_ID, 3, |_: Round| *PROPOSER_ID, false, quorum_type);

    wrapper.start();
    // Waiting for the proposal.
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPropose(ROUND));
    assert!(wrapper.next_event().is_none());
    wrapper.send_proposal(PROPOSAL_ID, ROUND);

    // The node says this proposal is valid (vote 1).
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Prevote(PROPOSAL_ID, ROUND));
    assert!(wrapper.next_event().is_none());

    // Another node sends a Prevote (vote 2).
    wrapper.send_prevote(PROPOSAL_ID, ROUND);

    // Byzantine quorum requires 3 votes, so we need one more vote.
    if quorum_type == QuorumType::Byzantine {
        // Not enough votes for a quorum yet.
        assert!(wrapper.next_event().is_none());

        // Another node sends a Prevote (vote 3).
        wrapper.send_prevote(PROPOSAL_ID, ROUND);
    }
    // In honest case, the second vote is enough for a quorum.

    // The Node got a Prevote quorum.
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPrevote(ROUND));

    // The Node sends a Precommit (vote 1).
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Precommit(PROPOSAL_ID, ROUND));
    assert!(wrapper.next_event().is_none());

    // Another node sends a Precommit (vote 2).
    wrapper.send_precommit(PROPOSAL_ID, ROUND);

    // Byzantine quorum requires 3 votes, so we need one more vote.
    if quorum_type == QuorumType::Byzantine {
        // Not enough votes for a quorum yet.
        assert!(wrapper.next_event().is_none());

        // Another node sends a Precommit (vote 3).
        wrapper.send_precommit(PROPOSAL_ID, ROUND);
    }
    // In honest case, the second vote is enough for a quorum.

    // The Node got a Precommit quorum.
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::TimeoutPrecommit(ROUND));
    assert_eq!(
        wrapper.next_event().unwrap(),
        StateMachineEvent::Decision(PROPOSAL_ID.unwrap(), ROUND)
    );
    assert!(wrapper.next_event().is_none());
}
