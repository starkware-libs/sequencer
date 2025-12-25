use apollo_consensus_config::config::TimeoutsConfig;
use apollo_protobuf::consensus::{ProposalInit, VoteType, DEFAULT_VALIDATOR_ID};
use assert_matches::assert_matches;
use lazy_static::lazy_static;
use starknet_api::block::BlockNumber;
use starknet_types_core::felt::Felt;
use test_case::test_case;

use super::SingleHeightConsensus;
use crate::state_machine::{SMRequest, StateMachineEvent, Step};
use crate::test_utils::{precommit, prevote, TestBlock};
use crate::types::{ProposalCommitment, Round, ValidatorId};
use crate::votes_threshold::QuorumType;

lazy_static! {
    static ref PROPOSER_ID: ValidatorId = DEFAULT_VALIDATOR_ID.into();
    static ref VALIDATOR_ID_1: ValidatorId = (DEFAULT_VALIDATOR_ID + 1).into();
    static ref VALIDATOR_ID_2: ValidatorId = (DEFAULT_VALIDATOR_ID + 2).into();
    static ref VALIDATOR_ID_3: ValidatorId = (DEFAULT_VALIDATOR_ID + 3).into();
    static ref VALIDATORS: Vec<ValidatorId> =
        vec![*PROPOSER_ID, *VALIDATOR_ID_1, *VALIDATOR_ID_2, *VALIDATOR_ID_3];
    static ref BLOCK: TestBlock =
        TestBlock { content: vec![1, 2, 3], id: ProposalCommitment(Felt::ONE) };
    static ref PROPOSAL_INIT: ProposalInit =
        ProposalInit { proposer: *PROPOSER_ID, ..Default::default() };
    static ref TIMEOUTS: TimeoutsConfig = TimeoutsConfig::default();
}
const HEIGHT_0: BlockNumber = BlockNumber(0);
const ROUND_0: Round = 0;
const ROUND_1: Round = 1;

fn get_proposal_init_for_height(height: BlockNumber) -> ProposalInit {
    ProposalInit { height, ..*PROPOSAL_INIT }
}

#[test]
fn proposer() {
    let mut shc = SingleHeightConsensus::new(
        HEIGHT_0,
        false,
        *PROPOSER_ID,
        VALIDATORS.to_vec(),
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
    );
    let leader_fn = |_round| -> ValidatorId { *PROPOSER_ID };
    // Start should request to build proposal.
    let start_ret = shc.start(&leader_fn);
    assert_matches!(start_ret, mut reqs => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::StartBuildProposal(ROUND_0)));
        assert!(reqs.is_empty());
    });

    // After FinishedBuilding, expect a prevote broadcast request.
    let ret =
        shc.handle_event(&leader_fn, StateMachineEvent::FinishedBuilding(Some(BLOCK.id), ROUND_0));
    assert_matches!(ret, mut reqs => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Prevote);
        assert!(reqs.is_empty());
    });

    // Receive two prevotes from other validators to reach prevote quorum.
    let _ =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_1));
    let ret =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    // Expect a precommit broadcast request present.
    assert_matches!(ret, mut reqs => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::ScheduleTimeout(Step::Prevote, ROUND_0)));
        assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Precommit);
        assert!(reqs.is_empty());
    });

    // Now provide precommit votes to reach decision.
    let _ = shc
        .handle_vote(&leader_fn, precommit(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_1));
    let decision = shc
        .handle_vote(&leader_fn, precommit(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    assert_matches!(decision, mut reqs => {
        assert_matches!(
            reqs.pop_front(),
            Some(SMRequest::ScheduleTimeout(Step::Precommit, ROUND_0))
        );
        assert_matches!(
            reqs.pop_front(),
            Some(SMRequest::DecisionReached(dec)) if dec.block == BLOCK.id
        );
        assert!(reqs.is_empty());
    });
}

#[test_case(false; "single_proposal")]
#[test_case(true; "repeat_proposal")]
fn validator(repeat_proposal: bool) {
    let proposal_init = get_proposal_init_for_height(HEIGHT_0);
    let mut shc = SingleHeightConsensus::new(
        HEIGHT_0,
        false,
        *VALIDATOR_ID_1,
        VALIDATORS.to_vec(),
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
    );
    let leader_fn = |_round| -> ValidatorId { *PROPOSER_ID };

    // Accept init -> should request validation.
    let ret = shc.handle_proposal(&leader_fn, proposal_init);
    assert_matches!(ret, mut reqs => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::StartValidateProposal(init)) if init == proposal_init);
        assert!(reqs.is_empty());
    });

    // After validation finished -> expect prevote broadcast request.
    let ret = shc.handle_event(
        &leader_fn,
        StateMachineEvent::FinishedValidation(Some(BLOCK.id), proposal_init.round, None),
    );
    assert_matches!(ret, mut reqs => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Prevote);
        assert!(reqs.is_empty());
    });

    if repeat_proposal {
        // Duplicate proposal init should be ignored.
        let ret = shc.handle_proposal(&leader_fn, proposal_init);
        assert!(matches!(ret, rs if rs.is_empty()));
    }

    // Reach prevote quorum with two other validators.
    let _ =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    let ret =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_3));
    // Expect a precommit broadcast request present.
    assert_matches!(ret, mut reqs => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::ScheduleTimeout(Step::Prevote, 0)));
        assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Precommit);
        assert!(reqs.is_empty());
    });

    // Now provide precommit votes to reach decision.
    let _ =
        shc.handle_vote(&leader_fn, precommit(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *PROPOSER_ID));
    let decision = shc
        .handle_vote(&leader_fn, precommit(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    assert_matches!(decision, mut reqs => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::ScheduleTimeout(Step::Precommit, ROUND_0)));
        assert_matches!(
            reqs.pop_front(),
            Some(SMRequest::DecisionReached(dec)) if dec.block == BLOCK.id
        );
        assert!(reqs.is_empty());
    });
}

#[test_case(true; "repeat")]
#[test_case(false; "equivocation")]
fn vote_twice(same_vote: bool) {
    let proposal_init = get_proposal_init_for_height(HEIGHT_0);
    let mut shc = SingleHeightConsensus::new(
        HEIGHT_0,
        false,
        *VALIDATOR_ID_1,
        VALIDATORS.to_vec(),
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
    );
    let leader_fn = |_round| -> ValidatorId { *PROPOSER_ID };
    // Validate a proposal so the SM is ready to prevote.
    shc.handle_proposal(&leader_fn, proposal_init);
    shc.handle_event(
        &leader_fn,
        StateMachineEvent::FinishedValidation(Some(BLOCK.id), proposal_init.round, None),
    );

    let _ = shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *PROPOSER_ID));
    let res =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    // On quorum of prevotes, expect a precommit broadcast request.
    assert_matches!(res, mut reqs => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::ScheduleTimeout(Step::Prevote, 0)));
        assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Precommit);
        assert!(reqs.is_empty());
    });

    // Precommit handling towards decision.
    let first_vote = precommit(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *PROPOSER_ID);
    let _ = shc.handle_vote(&leader_fn, first_vote.clone());
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
    let res = shc.handle_vote(&leader_fn, second_vote.clone());
    assert_matches!(res, r if r.is_empty());
    let decision = shc
        .handle_vote(&leader_fn, precommit(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_3));
    assert_matches!(decision, mut reqs => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::ScheduleTimeout(Step::Precommit, ROUND_0)));
        assert_matches!(
            reqs.pop_front(),
            Some(SMRequest::DecisionReached(dec)) if dec.block == BLOCK.id
        );
        assert!(reqs.is_empty());
    });
}

#[test]
fn rebroadcast_votes() {
    let mut shc = SingleHeightConsensus::new(
        HEIGHT_0,
        false,
        *PROPOSER_ID,
        VALIDATORS.to_vec(),
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
    );
    let leader_fn = |_round| -> ValidatorId { *PROPOSER_ID };
    // Start and build.
    let _ = shc.start(&leader_fn);

    let ret =
        shc.handle_event(&leader_fn, StateMachineEvent::FinishedBuilding(Some(BLOCK.id), ROUND_0));
    assert_matches!(ret, mut reqs => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Prevote);
        assert!(reqs.is_empty());
    });

    // Receive two prevotes from other validators to reach prevote quorum at round 0.
    let _ =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_1));
    let ret =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    // Expect a precommit broadcast at round 0.
    assert_matches!(ret, mut reqs => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::ScheduleTimeout(Step::Prevote, ROUND_0)));
        assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Precommit && v.round == ROUND_0);
        assert!(reqs.is_empty());
    });

    // Advance with NIL precommits from peers (no decision) -> expect scheduling of precommit
    // timeout.
    let _ = shc.handle_vote(&leader_fn, precommit(None, HEIGHT_0, ROUND_0, *VALIDATOR_ID_1));
    let _ = shc.handle_vote(&leader_fn, precommit(None, HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));

    // Timeout at precommit(0) -> expect a prevote broadcast for round 1.
    let ret = shc.handle_event(&leader_fn, StateMachineEvent::TimeoutPrecommit(ROUND_0));
    assert_matches!(ret, mut reqs => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::Repropose(proposal_id, init)) if proposal_id == BLOCK.id && init.round == ROUND_1 && init.valid_round == Some(ROUND_0));
        assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Prevote && v.round == ROUND_1);
        assert!(reqs.is_empty());
    });

    // Reach prevote quorum at round 1 with two other validators.
    let _ =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_1, *VALIDATOR_ID_2));
    let ret =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_1, *VALIDATOR_ID_3));
    assert_matches!(ret, mut reqs => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::ScheduleTimeout(Step::Prevote, ROUND_1)));
        assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Precommit && v.round == ROUND_1);
        assert!(reqs.is_empty());
    });

    // Rebroadcast with older vote (round 0) - should be ignored (no broadcast, no task).
    let ret = shc.handle_event(
        &leader_fn,
        StateMachineEvent::VoteBroadcasted(precommit(
            Some(BLOCK.id.0),
            HEIGHT_0,
            ROUND_0,
            *PROPOSER_ID,
        )),
    );
    assert_matches!(ret, r if r.is_empty());

    // Rebroadcast with current round (round 1) - should broadcast.
    let rebroadcast_vote = precommit(Some(BLOCK.id.0), HEIGHT_0, ROUND_1, *PROPOSER_ID);
    let ret =
        shc.handle_event(&leader_fn, StateMachineEvent::VoteBroadcasted(rebroadcast_vote.clone()));
    assert_matches!(ret, mut reqs => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v == rebroadcast_vote);
        assert!(reqs.is_empty());
    });
}

#[test]
fn repropose() {
    let mut shc = SingleHeightConsensus::new(
        HEIGHT_0,
        false,
        *PROPOSER_ID,
        VALIDATORS.to_vec(),
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
    );
    let leader_fn = |_round| -> ValidatorId { *PROPOSER_ID };
    let _ = shc.start(&leader_fn);
    // After building the proposal, the proposer broadcasts a prevote for round 0.
    let ret =
        shc.handle_event(&leader_fn, StateMachineEvent::FinishedBuilding(Some(BLOCK.id), ROUND_0));
    // Expect a BroadcastVote(Prevote) request for round 0.
    assert_matches!(ret, mut reqs => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Prevote && v.round == ROUND_0);
        assert!(reqs.is_empty());
    });
    // A single prevote from another validator does not yet cause quorum.
    let ret =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_1));
    // No new requests are expected at this point.
    assert_matches!(ret, reqs if reqs.is_empty());
    // Reaching prevote quorum with a second external prevote; proposer will broadcast a precommit
    // and schedule a prevote timeout for round 0.
    let ret =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    // Expect ScheduleTimeout(Step::Prevote, 0) and BroadcastVote(Precommit).
    assert_matches!(ret, mut reqs => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::ScheduleTimeout(Step::Prevote, ROUND_0)));
        assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Precommit && v.round == ROUND_0);
        assert!(reqs.is_empty());
    });
    // receiving Nil precommit requests and then decision on new round; just assert no panic and
    // decisions arrive after quorum.
    let _ = shc.handle_vote(&leader_fn, precommit(None, HEIGHT_0, ROUND_0, *VALIDATOR_ID_1));
    let ret = shc.handle_vote(&leader_fn, precommit(None, HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    // assert that ret is ScheduleTimeoutPrecommit
    assert_matches!(ret, mut reqs => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::ScheduleTimeout(Step::Precommit, ROUND_0)));
        assert!(reqs.is_empty());
    });
    // No precommit quorum is reached. On TimeoutPrecommit(0) the proposer advances to round 1 with
    // a valid value (valid_round = Some(0)) and reproposes the same block, then broadcasts a
    // new prevote for round 1.
    let ret = shc.handle_event(&leader_fn, StateMachineEvent::TimeoutPrecommit(ROUND_0));
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
    let mut shc = SingleHeightConsensus::new(
        BlockNumber(0),
        false,
        *PROPOSER_ID,
        VALIDATORS.to_vec(),
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
    );
    let leader_fn = |_round| -> ValidatorId { *PROPOSER_ID };
    let ret = shc.start(&leader_fn);
    assert_matches!(ret, mut reqs => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::StartBuildProposal(ROUND_0)));
        assert!(reqs.is_empty());
    });

    // Receive enough identical prevotes during awaiting_finished_building to trigger Timeout
    // (if they weren't duplicates)
    let duplicate_vote = prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_1);

    // First vote gets queued
    assert_matches!(
        shc.handle_vote(&leader_fn, duplicate_vote.clone()),
        reqs if reqs.is_empty()
    );

    // Remaining votes are duplicates - should be ignored
    for _ in 1..VALIDATORS.len() {
        assert_matches!(
            shc.handle_vote(&leader_fn, duplicate_vote.clone()),
            reqs if reqs.is_empty()
        );
    }

    // Finish building - processes the queue
    // Only one vote was queued (duplicates were ignored), so no TimeoutPrevote should be triggered,
    // only a broadcast vote
    let ret =
        shc.handle_event(&leader_fn, StateMachineEvent::FinishedBuilding(Some(BLOCK.id), ROUND_0));
    assert_matches!(ret, mut reqs => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Prevote && v.round == ROUND_0);
        assert!(reqs.is_empty());
    });
}

#[test]
fn broadcast_vote_before_decision_on_validation_finish() {
    let proposal_init = get_proposal_init_for_height(HEIGHT_0);
    let mut shc = SingleHeightConsensus::new(
        HEIGHT_0,
        false,
        *VALIDATOR_ID_1,
        VALIDATORS.to_vec(),
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
    );
    let leader_fn = |_round| -> ValidatorId { *PROPOSER_ID };

    // 1. Accept proposal -> should request validation
    let ret = shc.handle_proposal(&leader_fn, proposal_init);
    assert_matches!(ret, mut reqs => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::StartValidateProposal(init)) if init == proposal_init);
        assert!(reqs.is_empty());
    });

    // 2. Node receives 2/3 valid prevotes from others (3 out of 4 = 2/3)
    let _ = shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *PROPOSER_ID));
    let _ =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    let _ =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_3));

    // 3. Node receives 2/3-1 valid precommits + 1 nil precommit.
    // This triggers timeout precommit scheduling
    let _ =
        shc.handle_vote(&leader_fn, precommit(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *PROPOSER_ID));
    let _ = shc
        .handle_vote(&leader_fn, precommit(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    let ret = shc.handle_vote(&leader_fn, precommit(None, HEIGHT_0, ROUND_0, *VALIDATOR_ID_3));
    // Should schedule timeout precommit
    assert_matches!(ret, mut reqs => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::ScheduleTimeout(Step::Precommit, ROUND_0)));
        assert!(reqs.is_empty());
    });

    // 4. Before timeout precommit, validation finishes
    // 5. When validation finishes, state machine should see:
    //    - 2/3 prevotes with valid proposal (should vote precommit)
    //    - 2/3 precommits (with our precommit, should reach decision)
    // 6. Should return BOTH BroadcastVote (precommit) and DecisionReached
    let ret = shc.handle_event(
        &leader_fn,
        StateMachineEvent::FinishedValidation(Some(BLOCK.id), proposal_init.round, None),
    );
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
