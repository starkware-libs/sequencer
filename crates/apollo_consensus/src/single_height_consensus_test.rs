use std::collections::VecDeque;

use apollo_consensus_config::config::TimeoutsConfig;
use apollo_protobuf::consensus::{BuildParam, VoteType, DEFAULT_VALIDATOR_ID};
use assert_matches::assert_matches;
use lazy_static::lazy_static;
use starknet_api::block::BlockNumber;
use starknet_types_core::felt::Felt;
use test_case::test_case;

use super::SingleHeightConsensus;
use crate::state_machine::{SMRequest, StateMachineEvent, Step};
use crate::test_utils::{
    mock_committee_virtual_equal_to_actual,
    precommit,
    prevote,
    proposal_init,
    TestBlock,
};
use crate::types::{ProposalCommitment, Round, ValidatorId};
use crate::votes_threshold::QuorumType;

lazy_static! {
    static ref PROPOSER_ID: ValidatorId = DEFAULT_VALIDATOR_ID.into();
    static ref VALIDATOR_ID_1: ValidatorId = (DEFAULT_VALIDATOR_ID + 1).into();
    static ref VALIDATOR_ID_2: ValidatorId = (DEFAULT_VALIDATOR_ID + 2).into();
    static ref VALIDATOR_ID_3: ValidatorId = (DEFAULT_VALIDATOR_ID + 3).into();
    static ref VALIDATORS: Vec<ValidatorId> =
        vec![*PROPOSER_ID, *VALIDATOR_ID_1, *VALIDATOR_ID_2, *VALIDATOR_ID_3];
    /// Not in VALIDATORS; used for handle_vote_ignores_non_validator test.
    static ref NON_VALIDATOR_ID: ValidatorId = (DEFAULT_VALIDATOR_ID + 10).into();
    static ref BLOCK: TestBlock =
        TestBlock { content: vec![1, 2, 3], id: ProposalCommitment(Felt::ONE) };
    static ref BLOCK_B: ProposalCommitment = ProposalCommitment(Felt::TWO);
    static ref BUILD_PARAM: BuildParam =
        BuildParam { proposer: *PROPOSER_ID, ..Default::default() };
    static ref TIMEOUTS: TimeoutsConfig = TimeoutsConfig::default();
}
const HEIGHT_0: BlockNumber = BlockNumber(0);
const ROUND_0: Round = 0;
const ROUND_1: Round = 1;
const ROUND_2: Round = 2;
/// When true, require the virtual proposer to have voted in favor before reaching a decision.
const REQUIRE_VIRTUAL_PROPOSER_VOTE: bool = true;

fn new_shc(id: ValidatorId, is_observer: bool) -> SingleHeightConsensus {
    SingleHeightConsensus::new(
        HEIGHT_0,
        is_observer,
        id,
        VALIDATORS.to_vec(),
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
        mock_committee_virtual_equal_to_actual(
            VALIDATORS.to_vec(),
            Box::new(|_round| *PROPOSER_ID),
        ),
        REQUIRE_VIRTUAL_PROPOSER_VOTE,
    )
}

#[track_caller]
fn assert_start_build_proposal(reqs: &mut VecDeque<SMRequest>) {
    assert_matches!(reqs.pop_front(), Some(SMRequest::ScheduleTimeout(Step::Propose, ROUND_0)));
    assert_matches!(reqs.pop_front(), Some(SMRequest::StartBuildProposal(ROUND_0)));
    assert!(reqs.is_empty(), "unexpected requests: {:?}", reqs);
}

#[track_caller]
fn assert_prevote_broadcast(reqs: &mut VecDeque<SMRequest>, round: Round) {
    assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Prevote && v.round == round);
    assert!(reqs.is_empty(), "unexpected requests: {:?}", reqs);
}

#[track_caller]
fn assert_prevote_quorum_response(reqs: &mut VecDeque<SMRequest>, round: Round) {
    assert_matches!(reqs.pop_front(), Some(SMRequest::ScheduleTimeout(Step::Prevote, r)) if r == round);
    assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Precommit);
    assert!(reqs.is_empty(), "unexpected requests: {:?}", reqs);
}

#[track_caller]
fn assert_precommit_timeout_scheduled(reqs: &mut VecDeque<SMRequest>, round: Round) {
    assert_matches!(reqs.pop_front(), Some(SMRequest::ScheduleTimeout(Step::Precommit, r)) if r == round);
    assert!(reqs.is_empty(), "unexpected requests: {:?}", reqs);
}

#[track_caller]
fn assert_decision(
    reqs: &mut VecDeque<SMRequest>,
    round: Round,
    expected_block: ProposalCommitment,
) {
    assert_matches!(reqs.pop_front(), Some(SMRequest::ScheduleTimeout(Step::Precommit, r)) if r == round);
    assert_matches!(reqs.pop_front(), Some(SMRequest::DecisionReached(dec)) if dec.block == expected_block);
    assert!(reqs.is_empty(), "unexpected requests: {:?}", reqs);
}

#[test]
fn proposer() {
    let mut shc = new_shc(*PROPOSER_ID, false);
    // Start should request to build proposal.
    let mut start_ret = shc.start();
    assert_start_build_proposal(&mut start_ret);

    // After FinishedBuilding, expect a prevote broadcast request.
    let mut ret = shc.handle_event(StateMachineEvent::FinishedBuilding(Some(BLOCK.id), ROUND_0));
    assert_prevote_broadcast(&mut ret, ROUND_0);

    // Receive two prevotes from other validators to reach prevote quorum.
    let _ = shc.handle_vote(prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_1));
    let mut ret = shc.handle_vote(prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    assert_prevote_quorum_response(&mut ret, ROUND_0);

    // Now provide precommit votes to reach decision.
    let _ = shc.handle_vote(precommit(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_1));
    let mut decision =
        shc.handle_vote(precommit(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    assert_decision(&mut decision, ROUND_0, BLOCK.id);
}

#[test_case(false; "single_proposal")]
#[test_case(true; "repeat_proposal")]
fn validator(repeat_proposal: bool) {
    let mut shc = new_shc(*VALIDATOR_ID_1, false);

    // Accept init -> should request validation.
    let init = proposal_init(HEIGHT_0, ROUND_0, *PROPOSER_ID);
    let round = init.round;
    let ret = shc.handle_proposal(init.clone());
    assert_matches!(ret, mut reqs => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::StartValidateProposal(info)) if info == init);
        assert!(reqs.is_empty());
    });

    // After validation finished -> expect prevote broadcast request.
    let mut ret =
        shc.handle_event(StateMachineEvent::FinishedValidation(Some(BLOCK.id), round, None));
    assert_prevote_broadcast(&mut ret, ROUND_0);

    if repeat_proposal {
        // Duplicate block info should be ignored.
        let ret = shc.handle_proposal(init.clone());
        assert!(matches!(ret, rs if rs.is_empty()));
    }

    // Reach prevote quorum with two other validators.
    let _ = shc.handle_vote(prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    // Virtual leader (PROPOSER_ID) must be in favor of the block for the quorum to be accepted.
    let mut ret = shc.handle_vote(prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *PROPOSER_ID));
    assert_prevote_quorum_response(&mut ret, ROUND_0);

    // Now provide precommit votes to reach decision.
    let _ = shc.handle_vote(precommit(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *PROPOSER_ID));
    let mut decision =
        shc.handle_vote(precommit(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    assert_decision(&mut decision, ROUND_0, BLOCK.id);
}

#[test_case(true; "repeat")]
#[test_case(false; "equivocation")]
fn vote_twice(same_vote: bool) {
    let mut shc = new_shc(*VALIDATOR_ID_1, false);
    let init = proposal_init(HEIGHT_0, ROUND_0, *PROPOSER_ID);
    let round = init.round;
    shc.handle_proposal(init);
    shc.handle_event(StateMachineEvent::FinishedValidation(Some(BLOCK.id), round, None));

    let _ = shc.handle_vote(prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *PROPOSER_ID));
    let mut res = shc.handle_vote(prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    assert_prevote_quorum_response(&mut res, ROUND_0);

    // Precommit handling towards decision.
    let first_vote = precommit(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *PROPOSER_ID);
    let _ = shc.handle_vote(first_vote.clone());
    let second_vote = if same_vote {
        first_vote.clone()
    } else {
        precommit(Some(Felt::TWO), HEIGHT_0, ROUND_0, *PROPOSER_ID)
    };
    // When same_vote is true, this is a duplicate precommit from PROPOSER_ID (same as first_vote).
    // When same_vote is false, this is an equivocation (different vote from PROPOSER_ID).
    // Both cases should be ignored (return empty requests). After the first_vote, we have:
    // - VALIDATOR_ID_1 (self, broadcast when prevote quorum was reached)
    // - PROPOSER_ID (first_vote)
    // The second_vote from PROPOSER_ID is ignored, so we still need one more vote to reach
    // decision.
    let res = shc.handle_vote(second_vote.clone());
    assert_matches!(res, r if r.is_empty());
    let mut decision =
        shc.handle_vote(precommit(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_3));
    assert_decision(&mut decision, ROUND_0, BLOCK.id);
}

#[test]
fn rebroadcast_votes() {
    let mut shc = new_shc(*PROPOSER_ID, false);
    let _ = shc.start();
    let mut ret = shc.handle_event(StateMachineEvent::FinishedBuilding(Some(BLOCK.id), ROUND_0));
    assert_prevote_broadcast(&mut ret, ROUND_0);

    // Receive two prevotes from other validators to reach prevote quorum at round 0.
    let _ = shc.handle_vote(prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_1));
    let mut ret = shc.handle_vote(prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    assert_prevote_quorum_response(&mut ret, ROUND_0);

    // Advance with NIL precommits from peers (no decision) -> expect scheduling of precommit
    // timeout.
    let _ = shc.handle_vote(precommit(None, HEIGHT_0, ROUND_0, *VALIDATOR_ID_1));
    let _ = shc.handle_vote(precommit(None, HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));

    // Timeout at precommit(0) -> expect a prevote broadcast for round 1.
    let ret = shc.handle_event(StateMachineEvent::TimeoutPrecommit(ROUND_0));
    assert_matches!(ret, mut reqs => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::Repropose(proposal_id, init)) if proposal_id == BLOCK.id && init.round == ROUND_1 && init.valid_round == Some(ROUND_0));
        assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Prevote && v.round == ROUND_1);
        assert!(reqs.is_empty());
    });

    // Reach prevote quorum at round 1 with two other validators.
    let _ = shc.handle_vote(prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_1, *VALIDATOR_ID_2));
    let mut ret = shc.handle_vote(prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_1, *VALIDATOR_ID_3));
    assert_prevote_quorum_response(&mut ret, ROUND_1);

    // Rebroadcast with older vote (round 0) - should be ignored (no broadcast, no task).
    let ret = shc.handle_event(StateMachineEvent::VoteBroadcasted(precommit(
        Some(BLOCK.id.0),
        HEIGHT_0,
        ROUND_0,
        *PROPOSER_ID,
    )));
    assert_matches!(ret, r if r.is_empty());

    // Rebroadcast with current round (round 1) - should broadcast.
    let rebroadcast_vote = precommit(Some(BLOCK.id.0), HEIGHT_0, ROUND_1, *PROPOSER_ID);
    let ret = shc.handle_event(StateMachineEvent::VoteBroadcasted(rebroadcast_vote.clone()));
    assert_matches!(ret, mut reqs => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v == rebroadcast_vote);
        assert!(reqs.is_empty());
    });
}

#[test]
fn repropose() {
    let mut shc = new_shc(*PROPOSER_ID, false);
    let _ = shc.start();
    // After building the proposal, the proposer broadcasts a prevote for round 0.
    let mut ret = shc.handle_event(StateMachineEvent::FinishedBuilding(Some(BLOCK.id), ROUND_0));
    assert_prevote_broadcast(&mut ret, ROUND_0);
    // A single prevote from another validator does not yet cause quorum.
    let ret = shc.handle_vote(prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_1));
    // No new requests are expected at this point.
    assert_matches!(ret, reqs if reqs.is_empty());
    // Reaching prevote quorum with a second external prevote; proposer will broadcast a precommit
    // and schedule a prevote timeout for round 0.
    let mut ret = shc.handle_vote(prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    assert_prevote_quorum_response(&mut ret, ROUND_0);
    // receiving Nil precommit requests and then decision on new round; just assert no panic and
    // decisions arrive after quorum.
    let _ = shc.handle_vote(precommit(None, HEIGHT_0, ROUND_0, *VALIDATOR_ID_1));
    let mut ret = shc.handle_vote(precommit(None, HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    assert_precommit_timeout_scheduled(&mut ret, ROUND_0);
    // No precommit quorum is reached. On TimeoutPrecommit(0) the proposer advances to round 1 with
    // a valid value (valid_round = Some(0)) and reproposes the same block, then broadcasts a
    // new prevote for round 1.
    let ret = shc.handle_event(StateMachineEvent::TimeoutPrecommit(ROUND_0));
    // Expect Repropose with init.round == 1, init.valid_round == Some(0), and a
    // BroadcastVote(Prevote) for round 1.
    assert_matches!(ret, mut reqs => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::Repropose(proposal_id, init)) if proposal_id == BLOCK.id && init.round == ROUND_1 && init.valid_round == Some(ROUND_0));
        assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Prevote && v.round == ROUND_1);
        assert!(reqs.is_empty());
    });
}

#[tokio::test]
async fn duplicate_votes_during_awaiting_finished_building_are_ignored() {
    // This test verifies that receiving 3 identical prevotes during awaiting_finished_building
    // results in only one vote being processed, so no TimeoutPrevote is triggered.
    let mut shc = new_shc(*PROPOSER_ID, false);
    let mut ret = shc.start();
    assert_start_build_proposal(&mut ret);

    // Receive enough identical prevotes during awaiting_finished_building to trigger Timeout
    // (if they weren't duplicates)
    let duplicate_vote = prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_1);

    // First vote gets queued
    assert_matches!(
        shc.handle_vote( duplicate_vote.clone()),
        reqs if reqs.is_empty()
    );

    // Remaining votes are duplicates - should be ignored
    for _ in 1..VALIDATORS.len() {
        assert_matches!(
            shc.handle_vote( duplicate_vote.clone()),
            reqs if reqs.is_empty()
        );
    }

    // Finish building - processes the queue
    // Only one vote was queued (duplicates were ignored), so no TimeoutPrevote should be triggered,
    // only a broadcast vote
    let mut ret = shc.handle_event(StateMachineEvent::FinishedBuilding(Some(BLOCK.id), ROUND_0));
    assert_prevote_broadcast(&mut ret, ROUND_0);
}

#[test]
fn broadcast_vote_before_decision_on_validation_finish() {
    let mut shc = new_shc(*VALIDATOR_ID_1, false);
    // 1. Accept proposal -> should request validation
    let init = proposal_init(HEIGHT_0, ROUND_0, *PROPOSER_ID);
    let round = init.round;
    let ret = shc.handle_proposal(init);
    assert_matches!(ret, mut reqs => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::StartValidateProposal(_init)));
        assert!(reqs.is_empty());
    });

    // 2. Node receives 2/3 valid prevotes from others (3 out of 4 = 2/3)
    let _ = shc.handle_vote(prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *PROPOSER_ID));
    let _ = shc.handle_vote(prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    let _ = shc.handle_vote(prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_3));

    // 3. Node receives 2/3-1 valid precommits + 1 nil precommit.
    // This triggers timeout precommit scheduling
    let _ = shc.handle_vote(precommit(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *PROPOSER_ID));
    let _ = shc.handle_vote(precommit(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    let mut ret = shc.handle_vote(precommit(None, HEIGHT_0, ROUND_0, *VALIDATOR_ID_3));
    assert_precommit_timeout_scheduled(&mut ret, ROUND_0);

    // 4. Before timeout precommit, validation finishes
    // 5. When validation finishes, state machine should see:
    //    - 2/3 prevotes with valid proposal (should vote precommit)
    //    - 2/3 precommits (with our precommit, should reach decision)
    // 6. Should return BOTH BroadcastVote (precommit) and DecisionReached
    let ret = shc.handle_event(StateMachineEvent::FinishedValidation(Some(BLOCK.id), round, None));
    assert_matches!(ret, mut reqs => {
        assert_matches!(
            reqs.pop_front(),
            Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Prevote
                && v.proposal_commitment == Some(BLOCK.id)
                && v.voter == *VALIDATOR_ID_1
        );
        assert_matches!(
            reqs.pop_front(),
            Some(SMRequest::ScheduleTimeout(Step::Prevote, ROUND_0))
        );
        assert_matches!(
            reqs.pop_front(),
            Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Precommit
                && v.proposal_commitment == Some(BLOCK.id)
                && v.voter == *VALIDATOR_ID_1
        );
        assert_matches!(
            reqs.pop_front(),
            Some(SMRequest::DecisionReached(dec)) if dec.block == BLOCK.id
        );
        assert!(reqs.is_empty());
    });
}

#[test]
fn observer_does_not_broadcast_on_start_or_votes() {
    let mut shc = new_shc(*VALIDATOR_ID_1, true);

    let ret = shc.start();
    assert_matches!(ret, mut reqs => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::ScheduleTimeout(Step::Propose, ROUND_0)));
        assert!(reqs.is_empty());
    });

    let proposal_init = proposal_init(HEIGHT_0, ROUND_0, *PROPOSER_ID);
    let round = proposal_init.round;
    let _ = shc.handle_proposal(proposal_init);
    let ret = shc.handle_event(StateMachineEvent::FinishedValidation(Some(BLOCK.id), round, None));
    assert!(ret.is_empty());

    // Reach decision with peer prevotes/precommits only; observer should get DecisionReached but
    // never BroadcastVote.
    let _ = shc.handle_vote(prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *PROPOSER_ID));
    let _ = shc.handle_vote(prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    let _ = shc.handle_vote(prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_3));
    let _ = shc.handle_vote(precommit(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *PROPOSER_ID));
    let _ = shc.handle_vote(precommit(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    let mut decision =
        shc.handle_vote(precommit(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_3));
    assert_decision(&mut decision, ROUND_0, BLOCK.id);
}

#[test]
fn unlock_via_reproposal_with_valid_round() {
    // V1 is our node. We lock on A in round 0, miss the proposal in round 1 but see prevotes
    // for B, then in round 2 receive a re-proposal of B with valid_round=Some(1). We must
    // prevote B (LOC 28 unlock), not nil.
    let mut shc = new_shc(*VALIDATOR_ID_1, false);
    let peers = [*PROPOSER_ID, *VALIDATOR_ID_2, *VALIDATOR_ID_3];
    // Round 0: lock on A
    let init_a = proposal_init(HEIGHT_0, ROUND_0, *PROPOSER_ID);
    shc.handle_proposal(init_a);
    shc.handle_event(StateMachineEvent::FinishedValidation(Some(BLOCK.id), ROUND_0, None));
    shc.handle_vote(prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *PROPOSER_ID));
    let mut ret = shc.handle_vote(prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    // Self + P + V2 = 3 prevotes for A -> quorum (lock on A, broadcast precommit, schedule prevote
    // timeout).
    assert_prevote_quorum_response(&mut ret, ROUND_0);
    // Precommit timeout: no quorum for A; advance to round 1.
    for peer in peers {
        shc.handle_vote(precommit(None, HEIGHT_0, ROUND_0, peer));
    }
    shc.handle_event(StateMachineEvent::TimeoutPrecommit(ROUND_0));

    // Round 1: V1 never validates the proposal; inject prevotes for B from peers without locking
    // and nil precommits.
    shc.handle_event(StateMachineEvent::TimeoutPropose(ROUND_1));
    for peer in peers {
        shc.handle_vote(prevote(Some(BLOCK_B.0), HEIGHT_0, ROUND_1, peer));
        shc.handle_vote(precommit(None, HEIGHT_0, ROUND_1, peer));
    }
    shc.handle_event(StateMachineEvent::TimeoutPrecommit(ROUND_1));

    // Round 2: re-proposal of B with valid_round=1
    let mut init_b = proposal_init(HEIGHT_0, ROUND_2, *PROPOSER_ID);
    init_b.valid_round = Some(ROUND_1);
    let _ = shc.handle_proposal(init_b);
    let ret = shc.handle_event(StateMachineEvent::FinishedValidation(
        Some(*BLOCK_B),
        ROUND_2,
        Some(ROUND_1),
    ));
    // LOC 28 runs, we unlock and prevote B.
    assert_matches!(ret, mut reqs => {
        assert_matches!(
            reqs.pop_front(),
            Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Prevote
                && v.proposal_commitment == Some(*BLOCK_B)
                && v.voter == *VALIDATOR_ID_1
        );
        assert!(reqs.is_empty());
    });
}
