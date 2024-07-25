use lazy_static::lazy_static;
use starknet_api::block::BlockHash;
use starknet_types_core::felt::Felt;
use test_case::test_case;

use super::Round;
use crate::state_machine::{StateMachine, StateMachineEvent};
use crate::types::ValidatorId;

lazy_static! {
    static ref VALIDATOR_ID: ValidatorId = 1_u32.into();
    static ref PROPOSER_ID: ValidatorId = 0_u32.into();
}

const BLOCK_HASH: Option<BlockHash> = Some(BlockHash(Felt::ONE));
const ROUND: Round = 0;

#[test_case(true; "proposer")]
#[test_case(false; "validator")]
fn events_arrive_in_ideal_order(is_proposer: bool) {
    let id = if is_proposer { *PROPOSER_ID } else { *VALIDATOR_ID };
    let mut state_machine = StateMachine::new(id, 4);
    let leader_fn = |_: Round| *PROPOSER_ID;
    let mut events = state_machine.start(&leader_fn);
    if is_proposer {
        assert_eq!(events.pop_front().unwrap(), StateMachineEvent::GetProposal(None, ROUND));
        events = state_machine.handle_event(StateMachineEvent::GetProposal(BLOCK_HASH, ROUND));
        assert_eq!(events.pop_front().unwrap(), StateMachineEvent::Proposal(BLOCK_HASH, ROUND));
    } else {
        assert!(events.is_empty(), "{:?}", events);
        events = state_machine.handle_event(StateMachineEvent::Proposal(BLOCK_HASH, ROUND));
    }
    assert_eq!(events.pop_front().unwrap(), StateMachineEvent::Prevote(BLOCK_HASH, ROUND));
    assert!(events.is_empty(), "{:?}", events);

    events = state_machine.handle_event(StateMachineEvent::Prevote(BLOCK_HASH, ROUND));
    assert!(events.is_empty(), "{:?}", events);

    events = state_machine.handle_event(StateMachineEvent::Prevote(BLOCK_HASH, ROUND));
    assert_eq!(events.pop_front().unwrap(), StateMachineEvent::Precommit(BLOCK_HASH, ROUND));
    assert!(events.is_empty(), "{:?}", events);

    events = state_machine.handle_event(StateMachineEvent::Precommit(BLOCK_HASH, ROUND));
    assert!(events.is_empty(), "{:?}", events);

    events = state_machine.handle_event(StateMachineEvent::Precommit(BLOCK_HASH, ROUND));
    assert_eq!(
        events.pop_front().unwrap(),
        StateMachineEvent::Decision(BLOCK_HASH.unwrap(), ROUND)
    );
    assert!(events.is_empty(), "{:?}", events);
}

#[test]
fn validator_receives_votes_first() {
    let mut state_machine = StateMachine::new(*VALIDATOR_ID, 4);

    let leader_fn = |_: Round| *PROPOSER_ID;
    let mut events = state_machine.start(&leader_fn);
    assert!(events.is_empty(), "{:?}", events);

    // Receives votes from all the other nodes first (more than minimum for a quorum).
    events.append(&mut state_machine.handle_event(StateMachineEvent::Prevote(BLOCK_HASH, ROUND)));
    events.append(&mut state_machine.handle_event(StateMachineEvent::Prevote(BLOCK_HASH, ROUND)));
    events.append(&mut state_machine.handle_event(StateMachineEvent::Prevote(BLOCK_HASH, ROUND)));
    events.append(&mut state_machine.handle_event(StateMachineEvent::Precommit(BLOCK_HASH, ROUND)));
    events.append(&mut state_machine.handle_event(StateMachineEvent::Precommit(BLOCK_HASH, ROUND)));
    events.append(&mut state_machine.handle_event(StateMachineEvent::Precommit(BLOCK_HASH, ROUND)));
    assert!(events.is_empty(), "{:?}", events);

    // Finally the proposal arrives.
    events = state_machine.handle_event(StateMachineEvent::Proposal(BLOCK_HASH, ROUND));
    assert_eq!(events.pop_front().unwrap(), StateMachineEvent::Prevote(BLOCK_HASH, ROUND));
    assert_eq!(events.pop_front().unwrap(), StateMachineEvent::Precommit(BLOCK_HASH, ROUND));
    assert_eq!(
        events.pop_front().unwrap(),
        StateMachineEvent::Decision(BLOCK_HASH.unwrap(), ROUND)
    );
    assert!(events.is_empty(), "{:?}", events);
}
// TODO(Asmaa): Add this test when we support NIL votes.
// #[test]
// fn buffer_events_during_get_proposal() {
//     let mut state_machine = StateMachine::new(*PROPOSER_ID, 4);
//     let leader_fn = |_: Round| *PROPOSER_ID;
//     let mut events = state_machine.start(&leader_fn);
//     assert_eq!(events.pop_front().unwrap(), StateMachineEvent::GetProposal(None, 0));
//     assert!(events.is_empty(), "{:?}", events);

//     // TODO(matan): When we support NIL votes, we should send them. Real votes without the
// proposal     // doesn't make sense.
//     events.append(&mut state_machine.handle_event(StateMachineEvent::Proposal(BLOCK_HASH,
// ROUND)));     events.append(&mut
// state_machine.handle_event(StateMachineEvent::Prevote(BLOCK_HASH, ROUND)));     events.append(&
// mut state_machine.handle_event(StateMachineEvent::Prevote(BLOCK_HASH, ROUND)));     events.
// append(&mut state_machine.handle_event(StateMachineEvent::Prevote(BLOCK_HASH, ROUND)));
//     assert!(events.is_empty(), "{:?}", events);

//     // Node finishes building the proposal.
//     events = state_machine.handle_event(StateMachineEvent::GetProposal(None, 0));
//     assert_eq!(events.pop_front().unwrap(), StateMachineEvent::Prevote(BLOCK_HASH, ROUND));
//     assert_eq!(events.pop_front().unwrap(), StateMachineEvent::Precommit(BLOCK_HASH, ROUND));
//     assert!(events.is_empty(), "{:?}", events);
// }
