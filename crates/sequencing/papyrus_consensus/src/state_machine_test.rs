use std::collections::VecDeque;

use lazy_static::lazy_static;
use starknet_api::block::BlockHash;
use starknet_types_core::felt::Felt;
use test_case::test_case;

use super::Round;
use crate::state_machine::{StateMachine, StateMachineEvent};
use crate::types::ValidatorId;

lazy_static! {
    static ref PROPOSER_ID: ValidatorId = 0_u32.into();
    static ref VALIDATOR_ID: ValidatorId = 1_u32.into();
}

const BLOCK_HASH: Option<BlockHash> = Some(BlockHash(Felt::ONE));
const ROUND: Round = 0;

struct TestWrapper<LeaderFn: Fn(Round) -> ValidatorId> {
    state_machine: StateMachine,
    leader_fn: LeaderFn,
    events: VecDeque<StateMachineEvent>,
}

impl<LeaderFn: Fn(Round) -> ValidatorId> TestWrapper<LeaderFn> {
    pub fn new(id: ValidatorId, total_weight: u32, leader_fn: LeaderFn) -> Self {
        Self {
            state_machine: StateMachine::new(id, total_weight),
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

    pub fn send_get_proposal(&mut self, block_hash: Option<BlockHash>, round: Round) {
        self.send_event(StateMachineEvent::GetProposal(block_hash, round))
    }

    pub fn send_proposal(&mut self, block_hash: Option<BlockHash>, round: Round) {
        self.send_event(StateMachineEvent::Proposal(block_hash, round))
    }

    pub fn send_prevote(&mut self, block_hash: Option<BlockHash>, round: Round) {
        self.send_event(StateMachineEvent::Prevote(block_hash, round))
    }

    pub fn send_precommit(&mut self, block_hash: Option<BlockHash>, round: Round) {
        self.send_event(StateMachineEvent::Precommit(block_hash, round))
    }

    fn send_event(&mut self, event: StateMachineEvent) {
        self.events.append(&mut self.state_machine.handle_event(event, &self.leader_fn));
    }
}

#[test_case(true; "proposer")]
#[test_case(false; "validator")]
fn events_arrive_in_ideal_order(is_proposer: bool) {
    let id = if is_proposer { *PROPOSER_ID } else { *VALIDATOR_ID };
    let mut wrapper = TestWrapper::new(id, 4, |_: Round| *PROPOSER_ID);

    wrapper.start();
    if is_proposer {
        assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::GetProposal(None, ROUND));
        wrapper.send_get_proposal(BLOCK_HASH, ROUND);
        assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Proposal(BLOCK_HASH, ROUND));
    } else {
        assert!(wrapper.next_event().is_none());
        wrapper.send_proposal(BLOCK_HASH, ROUND);
    }
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Prevote(BLOCK_HASH, ROUND));
    assert!(wrapper.next_event().is_none());

    wrapper.send_prevote(BLOCK_HASH, ROUND);
    assert!(wrapper.next_event().is_none());

    wrapper.send_prevote(BLOCK_HASH, ROUND);
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Precommit(BLOCK_HASH, ROUND));
    assert!(wrapper.next_event().is_none());

    wrapper.send_precommit(BLOCK_HASH, ROUND);
    assert!(wrapper.next_event().is_none());

    wrapper.send_precommit(BLOCK_HASH, ROUND);
    assert_eq!(
        wrapper.next_event().unwrap(),
        StateMachineEvent::Decision(BLOCK_HASH.unwrap(), ROUND)
    );
    assert!(wrapper.next_event().is_none());
}

#[test]
fn validator_receives_votes_first() {
    let mut wrapper = TestWrapper::new(*VALIDATOR_ID, 4, |_: Round| *PROPOSER_ID);

    wrapper.start();
    assert!(wrapper.next_event().is_none());

    // Receives votes from all the other nodes first (more than minimum for a quorum).
    wrapper.send_prevote(BLOCK_HASH, ROUND);
    wrapper.send_prevote(BLOCK_HASH, ROUND);
    wrapper.send_prevote(BLOCK_HASH, ROUND);
    wrapper.send_precommit(BLOCK_HASH, ROUND);
    wrapper.send_precommit(BLOCK_HASH, ROUND);
    wrapper.send_precommit(BLOCK_HASH, ROUND);
    assert!(wrapper.next_event().is_none());

    // Finally the proposal arrives.
    wrapper.send_proposal(BLOCK_HASH, ROUND);
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Prevote(BLOCK_HASH, ROUND));
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Precommit(BLOCK_HASH, ROUND));
    assert_eq!(
        wrapper.next_event().unwrap(),
        StateMachineEvent::Decision(BLOCK_HASH.unwrap(), ROUND)
    );
    assert!(wrapper.next_event().is_none());
}

#[test_case(BLOCK_HASH ; "valid_proposal")]
#[test_case(None ; "invalid_proposal")]
fn buffer_events_during_get_proposal(vote: Option<BlockHash>) {
    let mut wrapper = TestWrapper::new(*PROPOSER_ID, 4, |_: Round| *PROPOSER_ID);

    wrapper.start();
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::GetProposal(None, 0));
    assert!(wrapper.next_event().is_none());

    wrapper.send_prevote(vote, ROUND);
    wrapper.send_prevote(vote, ROUND);
    wrapper.send_prevote(vote, ROUND);
    assert!(wrapper.next_event().is_none());

    // Node finishes building the proposal.
    wrapper.send_get_proposal(BLOCK_HASH, ROUND);
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Proposal(BLOCK_HASH, ROUND));
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Prevote(BLOCK_HASH, ROUND));
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Precommit(vote, ROUND));
    assert!(wrapper.next_event().is_none());
}

#[test]
fn only_send_precommit_with_prevote_quorum_and_proposal() {
    let mut wrapper = TestWrapper::new(*VALIDATOR_ID, 4, |_: Round| *PROPOSER_ID);

    wrapper.start();
    assert!(wrapper.next_event().is_none());

    // Receives votes from all the other nodes first (more than minimum for a quorum).
    wrapper.send_prevote(BLOCK_HASH, ROUND);
    wrapper.send_prevote(BLOCK_HASH, ROUND);
    wrapper.send_prevote(BLOCK_HASH, ROUND);
    assert!(wrapper.next_event().is_none());

    // Finally the proposal arrives.
    wrapper.send_proposal(BLOCK_HASH, ROUND);
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Prevote(BLOCK_HASH, ROUND));
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Precommit(BLOCK_HASH, ROUND));
    assert!(wrapper.next_event().is_none());
}

#[test]
fn only_decide_with_prcommit_quorum_and_proposal() {
    let mut wrapper = TestWrapper::new(*VALIDATOR_ID, 4, |_: Round| *PROPOSER_ID);

    wrapper.start();
    assert!(wrapper.next_event().is_none());

    // Receives votes from all the other nodes first (more than minimum for a quorum).
    wrapper.send_prevote(BLOCK_HASH, ROUND);
    wrapper.send_prevote(BLOCK_HASH, ROUND);
    wrapper.send_prevote(BLOCK_HASH, ROUND);
    wrapper.send_precommit(BLOCK_HASH, ROUND);
    wrapper.send_precommit(BLOCK_HASH, ROUND);
    assert!(wrapper.next_event().is_none());

    // Finally the proposal arrives.
    wrapper.send_proposal(BLOCK_HASH, ROUND);
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Prevote(BLOCK_HASH, ROUND));
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Precommit(BLOCK_HASH, ROUND));
    assert_eq!(
        wrapper.next_event().unwrap(),
        StateMachineEvent::Decision(BLOCK_HASH.unwrap(), ROUND)
    );
    assert!(wrapper.next_event().is_none());
}

#[test]
fn advance_to_the_next_round() {
    let mut wrapper = TestWrapper::new(*VALIDATOR_ID, 4, |_: Round| *PROPOSER_ID);

    wrapper.start();
    assert!(wrapper.next_event().is_none());

    wrapper.send_proposal(BLOCK_HASH, ROUND);
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Prevote(BLOCK_HASH, ROUND));
    wrapper.send_precommit(None, ROUND);
    wrapper.send_precommit(None, ROUND);
    assert!(wrapper.next_event().is_none());

    wrapper.send_proposal(BLOCK_HASH, ROUND + 1);
    assert!(wrapper.next_event().is_none());

    wrapper.send_precommit(None, ROUND);
    // The Node sends Prevote after advancing to the next round.
    assert_eq!(wrapper.next_event().unwrap(), StateMachineEvent::Prevote(BLOCK_HASH, ROUND + 1));
}
