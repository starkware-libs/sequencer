use apollo_consensus_config::config::TimeoutsConfig;
use apollo_protobuf::consensus::{ProposalInit, VoteType, DEFAULT_VALIDATOR_ID};
use assert_matches::assert_matches;
use lazy_static::lazy_static;
use starknet_api::block::BlockNumber;
use starknet_types_core::felt::Felt;
use test_case::test_case;

use super::SingleHeightConsensus;
use crate::single_height_consensus::ShcReturn;
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
    assert_matches!(start_ret, ShcReturn::Requests(mut reqs) => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::StartBuildProposal(ROUND_0)));
        assert!(reqs.is_empty());
    });

<<<<<<< HEAD
    let precommits = [
        precommit(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_1),
        precommit(Some(Felt::TWO), 0, 0, *VALIDATOR_ID_3),
        precommit(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_2),
        precommit(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID),
    ];
    assert_eq!(
        shc.handle_vote(&mut context, precommits[0].clone()).await,
        Ok(ShcReturn::Tasks(Vec::new()))
    );
    // The disagreeing vote counts towards the timeout, which uses a heterogeneous quorum, but not
    // the decision, which uses a homogenous quorum.
    assert_eq!(
        shc.handle_vote(&mut context, precommits[1].clone()).await,
        Ok(ShcReturn::Tasks(vec![timeout_precommit_task(0),]))
    );
    let ShcReturn::Decision(decision) =
        shc.handle_vote(&mut context, precommits[2].clone()).await.unwrap()
    else {
        panic!("Expected decision");
    };
    assert_eq!(decision.block, BLOCK.id);
    assert!(decision.precommits.into_iter().all(|item| precommits.contains(&item)));
||||||| dd2fc66ab
    let precommits = vec![
        precommit(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_1),
        precommit(Some(Felt::TWO), 0, 0, *VALIDATOR_ID_3),
        precommit(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_2),
        precommit(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID),
    ];
    assert_eq!(
        shc.handle_vote(&mut context, precommits[0].clone()).await,
        Ok(ShcReturn::Tasks(Vec::new()))
    );
    // The disagreeing vote counts towards the timeout, which uses a heterogeneous quorum, but not
    // the decision, which uses a homogenous quorum.
    assert_eq!(
        shc.handle_vote(&mut context, precommits[1].clone()).await,
        Ok(ShcReturn::Tasks(vec![timeout_precommit_task(0),]))
    );
    let ShcReturn::Decision(decision) =
        shc.handle_vote(&mut context, precommits[2].clone()).await.unwrap()
    else {
        panic!("Expected decision");
    };
    assert_eq!(decision.block, BLOCK.id);
    assert!(decision.precommits.into_iter().all(|item| precommits.contains(&item)));
=======
    // After FinishedBuilding, expect a prevote broadcast request.
    let ret =
        shc.handle_event(&leader_fn, StateMachineEvent::FinishedBuilding(Some(BLOCK.id), ROUND_0));
    assert_matches!(ret, ShcReturn::Requests(mut reqs) => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Prevote);
        assert!(reqs.is_empty());
    });

    // Receive two prevotes from other validators to reach prevote quorum.
    let _ =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_1));
    let ret =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    // Expect a precommit broadcast request present.
    assert_matches!(ret, ShcReturn::Requests(mut reqs) => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::ScheduleTimeout(Step::Prevote, ROUND_0)));
        assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Precommit);
        assert!(reqs.is_empty());
    });

    // Now provide precommit votes to reach decision.
    let _ = shc
        .handle_vote(&leader_fn, precommit(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_1));
    let decision = shc
        .handle_vote(&leader_fn, precommit(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    match decision {
        ShcReturn::Decision(d) => assert_eq!(d.block, BLOCK.id),
        _ => panic!("expected decision"),
    }
>>>>>>> origin/main-v0.14.1
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
    assert_matches!(ret, ShcReturn::Requests(mut reqs) => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::StartValidateProposal(init)) if init == proposal_init);
        assert!(reqs.is_empty());
    });

<<<<<<< HEAD
    let precommits = [
        precommit(Some(BLOCK.id.0), HEIGHT.0, 0, *PROPOSER_ID),
        precommit(Some(BLOCK.id.0), HEIGHT.0, 0, *VALIDATOR_ID_2),
        precommit(Some(BLOCK.id.0), HEIGHT.0, 0, *VALIDATOR_ID_1),
    ];
    assert_eq!(
        shc.handle_vote(&mut context, precommits[0].clone()).await,
        Ok(ShcReturn::Tasks(Vec::new()))
||||||| dd2fc66ab
    let precommits = vec![
        precommit(Some(BLOCK.id.0), HEIGHT.0, 0, *PROPOSER_ID),
        precommit(Some(BLOCK.id.0), HEIGHT.0, 0, *VALIDATOR_ID_2),
        precommit(Some(BLOCK.id.0), HEIGHT.0, 0, *VALIDATOR_ID_1),
    ];
    assert_eq!(
        shc.handle_vote(&mut context, precommits[0].clone()).await,
        Ok(ShcReturn::Tasks(Vec::new()))
=======
    // After validation finished -> expect prevote broadcast request.
    let ret = shc.handle_event(
        &leader_fn,
        StateMachineEvent::FinishedValidation(Some(BLOCK.id), proposal_init.round, None),
>>>>>>> origin/main-v0.14.1
    );
    assert_matches!(ret, ShcReturn::Requests(mut reqs) => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Prevote);
        assert!(reqs.is_empty());
    });

    if repeat_proposal {
        // Duplicate proposal init should be ignored.
        let ret = shc.handle_proposal(&leader_fn, proposal_init);
        assert!(matches!(ret, ShcReturn::Requests(rs) if rs.is_empty()));
    }

    // Reach prevote quorum with two other validators.
    let _ =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    let ret =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_3));
    // Expect a precommit broadcast request present.
    assert_matches!(ret, ShcReturn::Requests(mut reqs) => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::ScheduleTimeout(Step::Prevote, 0)));
        assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Precommit);
        assert!(reqs.is_empty());
    });

    // Now provide precommit votes to reach decision.
    let _ =
        shc.handle_vote(&leader_fn, precommit(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *PROPOSER_ID));
    let decision = shc
        .handle_vote(&leader_fn, precommit(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    match decision {
        ShcReturn::Decision(d) => assert_eq!(d.block, BLOCK.id),
        _ => panic!("expected decision"),
    }
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
    assert_matches!(res, ShcReturn::Requests(mut reqs) => {
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
    assert_matches!(res, ShcReturn::Requests(r) if r.is_empty());
    let decision = shc
        .handle_vote(&leader_fn, precommit(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_3));
    match decision {
        ShcReturn::Decision(d) => assert_eq!(d.block, BLOCK.id),
        _ => panic!("Expected decision"),
    }
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
    assert_matches!(ret, ShcReturn::Requests(mut reqs) => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Prevote);
        assert!(reqs.is_empty());
    });

    // Receive two prevotes from other validators to reach prevote quorum at round 0.
    let _ =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_1));
    let ret =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    // Expect a precommit broadcast at round 0.
    assert_matches!(ret, ShcReturn::Requests(mut reqs) => {
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
    assert_matches!(ret, ShcReturn::Requests(mut reqs) => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::Repropose(proposal_id, init)) if proposal_id == BLOCK.id && init.round == ROUND_1 && init.valid_round == Some(ROUND_0));
        assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Prevote && v.round == ROUND_1);
        assert!(reqs.is_empty());
    });
<<<<<<< HEAD
    context.expect_set_height_and_round().returning(move |_, _| ());
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &Vote| msg == &prevote(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID))
        .returning(move |_| Ok(()));
    // Sends proposal and prevote.
    shc.start(&mut context).await.unwrap();
    shc.handle_event(&mut context, StateMachineEvent::FinishedBuilding(Some(BLOCK.id), 0))
        .await
        .unwrap();
    shc.handle_vote(&mut context, prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_1)).await.unwrap();
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &Vote| msg == &precommit(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID))
        .returning(move |_| Ok(()));
    // The Node got a Prevote quorum, and set valid proposal.
    assert_eq!(
        shc.handle_vote(&mut context, prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_2)).await,
        Ok(ShcReturn::Tasks(vec![
            timeout_prevote_task(0),
            rebroadcast_precommit_task(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID),
        ]))
    );
    // Advance to the next round.
    let precommits = [
        precommit(None, 0, 0, *VALIDATOR_ID_1),
        precommit(None, 0, 0, *VALIDATOR_ID_2),
        precommit(None, 0, 0, *VALIDATOR_ID_3),
    ];
    shc.handle_vote(&mut context, precommits[0].clone()).await.unwrap();
    shc.handle_vote(&mut context, precommits[1].clone()).await.unwrap();
    // After NIL precommits, the proposer should re-propose.
    context.expect_repropose().returning(move |id, init| {
        assert_eq!(init.height, BlockNumber(0));
        assert_eq!(id, BLOCK.id);
||||||| dd2fc66ab
    context.expect_set_height_and_round().returning(move |_, _| ());
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &Vote| msg == &prevote(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID))
        .returning(move |_| Ok(()));
    // Sends proposal and prevote.
    shc.start(&mut context).await.unwrap();
    shc.handle_event(&mut context, StateMachineEvent::FinishedBuilding(Some(BLOCK.id), 0))
        .await
        .unwrap();
    shc.handle_vote(&mut context, prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_1)).await.unwrap();
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &Vote| msg == &precommit(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID))
        .returning(move |_| Ok(()));
    // The Node got a Prevote quorum, and set valid proposal.
    assert_eq!(
        shc.handle_vote(&mut context, prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_2)).await,
        Ok(ShcReturn::Tasks(vec![
            timeout_prevote_task(0),
            rebroadcast_precommit_task(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID),
        ]))
    );
    // Advance to the next round.
    let precommits = vec![
        precommit(None, 0, 0, *VALIDATOR_ID_1),
        precommit(None, 0, 0, *VALIDATOR_ID_2),
        precommit(None, 0, 0, *VALIDATOR_ID_3),
    ];
    shc.handle_vote(&mut context, precommits[0].clone()).await.unwrap();
    shc.handle_vote(&mut context, precommits[1].clone()).await.unwrap();
    // After NIL precommits, the proposer should re-propose.
    context.expect_repropose().returning(move |id, init| {
        assert_eq!(init.height, BlockNumber(0));
        assert_eq!(id, BLOCK.id);
=======

    // Reach prevote quorum at round 1 with two other validators.
    let _ =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_1, *VALIDATOR_ID_2));
    let ret =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_1, *VALIDATOR_ID_3));
    assert_matches!(ret, ShcReturn::Requests(mut reqs) => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::ScheduleTimeout(Step::Prevote, ROUND_1)));
        assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Precommit && v.round == ROUND_1);
        assert!(reqs.is_empty());
>>>>>>> origin/main-v0.14.1
    });

<<<<<<< HEAD
    let precommits = [
        precommit(Some(BLOCK.id.0), 0, 1, *VALIDATOR_ID_1),
        precommit(Some(BLOCK.id.0), 0, 1, *VALIDATOR_ID_2),
        precommit(Some(BLOCK.id.0), 0, 1, *VALIDATOR_ID_3),
    ];
    shc.handle_vote(&mut context, precommits[0].clone()).await.unwrap();
    shc.handle_vote(&mut context, precommits[1].clone()).await.unwrap();
    let ShcReturn::Decision(decision) =
        shc.handle_vote(&mut context, precommits[2].clone()).await.unwrap()
    else {
        panic!("Expected decision");
    };
    assert_eq!(decision.block, BLOCK.id);
    assert!(decision.precommits.into_iter().all(|item| precommits.contains(&item)));
}

#[tokio::test]
async fn writes_voted_height_to_storage() {
    const HEIGHT: BlockNumber = BlockNumber(123);

    let mock_storage = Arc::new(Mutex::new(MockHeightVotedStorageTrait::new()));
    let mut storage_before_broadcast_sequence = Sequence::new();

    let mut context = MockTestContext::new();

    context.expect_proposer().returning(move |_, _| *PROPOSER_ID);
    context.expect_validate_proposal().returning(move |_, _, _| {
        let (block_sender, block_receiver) = oneshot::channel();
        block_sender.send(BLOCK.id).unwrap();
        block_receiver
    });
    context.expect_set_height_and_round().returning(move |_, _| ());

    mock_storage
        .lock()
        .unwrap()
        .expect_set_prev_voted_height()
        .with(eq(HEIGHT))
        .times(1)
        .in_sequence(&mut storage_before_broadcast_sequence)
        .returning(move |_| Ok(()));

    context
        .expect_broadcast()
        .times(1) // Once we see the first broadcast we expect the voted height to be written to storage.
        .withf(move |msg: &Vote| msg == &prevote(Some(BLOCK.id.0), HEIGHT.0, 0, *VALIDATOR_ID_1))
        .in_sequence(&mut storage_before_broadcast_sequence)
        .returning(move |_| Ok(()));

    let mut shc = SingleHeightConsensus::new(
        HEIGHT,
        false,
        *VALIDATOR_ID_1,
        VALIDATORS.to_vec(),
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
        mock_storage.clone(),
    );

    let shc_ret = handle_proposal(HEIGHT, &mut shc, &mut context).await;
    assert_eq!(
        shc_ret.as_tasks().unwrap()[0].as_validate_proposal().unwrap().0,
        &get_proposal_init_for_height(HEIGHT)
    );
    assert_eq!(
        shc.handle_event(&mut context, VALIDATE_PROPOSAL_EVENT.clone()).await,
        Ok(ShcReturn::Tasks(vec![rebroadcast_prevote_task(
||||||| dd2fc66ab
    let precommits = vec![
        precommit(Some(BLOCK.id.0), 0, 1, *VALIDATOR_ID_1),
        precommit(Some(BLOCK.id.0), 0, 1, *VALIDATOR_ID_2),
        precommit(Some(BLOCK.id.0), 0, 1, *VALIDATOR_ID_3),
    ];
    shc.handle_vote(&mut context, precommits[0].clone()).await.unwrap();
    shc.handle_vote(&mut context, precommits[1].clone()).await.unwrap();
    let ShcReturn::Decision(decision) =
        shc.handle_vote(&mut context, precommits[2].clone()).await.unwrap()
    else {
        panic!("Expected decision");
    };
    assert_eq!(decision.block, BLOCK.id);
    assert!(decision.precommits.into_iter().all(|item| precommits.contains(&item)));
}

#[tokio::test]
async fn writes_voted_height_to_storage() {
    const HEIGHT: BlockNumber = BlockNumber(123);

    let mock_storage = Arc::new(Mutex::new(MockHeightVotedStorageTrait::new()));
    let mut storage_before_broadcast_sequence = Sequence::new();

    let mut context = MockTestContext::new();

    context.expect_proposer().returning(move |_, _| *PROPOSER_ID);
    context.expect_validate_proposal().returning(move |_, _, _| {
        let (block_sender, block_receiver) = oneshot::channel();
        block_sender.send(BLOCK.id).unwrap();
        block_receiver
    });
    context.expect_set_height_and_round().returning(move |_, _| ());

    mock_storage
        .lock()
        .unwrap()
        .expect_set_prev_voted_height()
        .with(eq(HEIGHT))
        .times(1)
        .in_sequence(&mut storage_before_broadcast_sequence)
        .returning(move |_| Ok(()));

    context
        .expect_broadcast()
        .times(1) // Once we see the first broadcast we expect the voted height to be written to storage.
        .withf(move |msg: &Vote| msg == &prevote(Some(BLOCK.id.0), HEIGHT.0, 0, *VALIDATOR_ID_1))
        .in_sequence(&mut storage_before_broadcast_sequence)
        .returning(move |_| Ok(()));

    let mut shc = SingleHeightConsensus::new(
        HEIGHT,
        false,
        *VALIDATOR_ID_1,
        VALIDATORS.to_vec(),
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
        mock_storage.clone(),
    );

    let shc_ret = handle_proposal(HEIGHT, &mut shc, &mut context).await;
    assert_eq!(
        shc_ret.as_tasks().unwrap()[0].as_validate_proposal().unwrap().0,
        &get_proposal_init_for_height(HEIGHT)
    );
    assert_eq!(
        shc.handle_event(&mut context, VALIDATE_PROPOSAL_EVENT.clone()).await,
        Ok(ShcReturn::Tasks(vec![rebroadcast_prevote_task(
=======
    // Rebroadcast with older vote (round 0) - should be ignored (no broadcast, no task).
    let ret = shc.handle_event(
        &leader_fn,
        StateMachineEvent::VoteBroadcasted(precommit(
>>>>>>> origin/main-v0.14.1
            Some(BLOCK.id.0),
            HEIGHT_0,
            ROUND_0,
            *PROPOSER_ID,
        )),
    );
    assert_matches!(ret, ShcReturn::Requests(r) if r.is_empty());

    // Rebroadcast with current round (round 1) - should broadcast.
    let rebroadcast_vote = precommit(Some(BLOCK.id.0), HEIGHT_0, ROUND_1, *PROPOSER_ID);
    let ret =
        shc.handle_event(&leader_fn, StateMachineEvent::VoteBroadcasted(rebroadcast_vote.clone()));
    assert_matches!(ret, ShcReturn::Requests(mut reqs) => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v == rebroadcast_vote);
        assert!(reqs.is_empty());
    });
}

<<<<<<< HEAD
    // Add enough prevotes so that we move to precommit step and set expectations for the precommit
    // broadcast and storage write:

    mock_storage
        .lock()
        .unwrap()
        .expect_set_prev_voted_height()
        .with(eq(HEIGHT))
        .times(1)
        .in_sequence(&mut storage_before_broadcast_sequence)
        .returning(move |_| Ok(()));
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &Vote| msg == &precommit(Some(BLOCK.id.0), HEIGHT.0, 0, *VALIDATOR_ID_1))
        .in_sequence(&mut storage_before_broadcast_sequence)
        .returning(move |_| Ok(()));
    // The Node got a Prevote quorum.
    assert_eq!(
        shc.handle_vote(&mut context, prevote(Some(BLOCK.id.0), HEIGHT.0, 0, *VALIDATOR_ID_2))
            .await,
        Ok(ShcReturn::Tasks(vec![
            timeout_prevote_task(0),
            rebroadcast_precommit_task(Some(BLOCK.id.0), HEIGHT.0, 0, *VALIDATOR_ID_1)
        ]))
    );

    let precommits = [
        precommit(Some(BLOCK.id.0), HEIGHT.0, 0, *PROPOSER_ID),
        precommit(Some(BLOCK.id.0), HEIGHT.0, 0, *VALIDATOR_ID_2),
        precommit(Some(BLOCK.id.0), HEIGHT.0, 0, *VALIDATOR_ID_1),
    ];
    assert_eq!(
        // This is the call that will result in the precommit broadcast and storage write.
        shc.handle_vote(&mut context, precommits[0].clone()).await,
        Ok(ShcReturn::Tasks(Vec::new()))
||||||| dd2fc66ab
    // Add enough prevotes so that we move to precommit step and set expectations for the precommit
    // broadcast and storage write:

    mock_storage
        .lock()
        .unwrap()
        .expect_set_prev_voted_height()
        .with(eq(HEIGHT))
        .times(1)
        .in_sequence(&mut storage_before_broadcast_sequence)
        .returning(move |_| Ok(()));
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &Vote| msg == &precommit(Some(BLOCK.id.0), HEIGHT.0, 0, *VALIDATOR_ID_1))
        .in_sequence(&mut storage_before_broadcast_sequence)
        .returning(move |_| Ok(()));
    // The Node got a Prevote quorum.
    assert_eq!(
        shc.handle_vote(&mut context, prevote(Some(BLOCK.id.0), HEIGHT.0, 0, *VALIDATOR_ID_2))
            .await,
        Ok(ShcReturn::Tasks(vec![
            timeout_prevote_task(0),
            rebroadcast_precommit_task(Some(BLOCK.id.0), HEIGHT.0, 0, *VALIDATOR_ID_1)
        ]))
    );

    let precommits = vec![
        precommit(Some(BLOCK.id.0), HEIGHT.0, 0, *PROPOSER_ID),
        precommit(Some(BLOCK.id.0), HEIGHT.0, 0, *VALIDATOR_ID_2),
        precommit(Some(BLOCK.id.0), HEIGHT.0, 0, *VALIDATOR_ID_1),
    ];
    assert_eq!(
        // This is the call that will result in the precommit broadcast and storage write.
        shc.handle_vote(&mut context, precommits[0].clone()).await,
        Ok(ShcReturn::Tasks(Vec::new()))
=======
#[test]
fn repropose() {
    let mut shc = SingleHeightConsensus::new(
        HEIGHT_0,
        false,
        *PROPOSER_ID,
        VALIDATORS.to_vec(),
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
>>>>>>> origin/main-v0.14.1
    );
    let leader_fn = |_round| -> ValidatorId { *PROPOSER_ID };
    let _ = shc.start(&leader_fn);
    // After building the proposal, the proposer broadcasts a prevote for round 0.
    let ret =
        shc.handle_event(&leader_fn, StateMachineEvent::FinishedBuilding(Some(BLOCK.id), ROUND_0));
    // Expect a BroadcastVote(Prevote) request for round 0.
    assert_matches!(ret, ShcReturn::Requests(mut reqs) => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Prevote && v.round == ROUND_0);
        assert!(reqs.is_empty());
    });
    // A single prevote from another validator does not yet cause quorum.
    let ret =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_1));
    // No new requests are expected at this point.
    assert_matches!(ret, ShcReturn::Requests(reqs) if reqs.is_empty());
    // Reaching prevote quorum with a second external prevote; proposer will broadcast a precommit
    // and schedule a prevote timeout for round 0.
    let ret =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    // Expect ScheduleTimeout(Step::Prevote, 0) and BroadcastVote(Precommit).
    assert_matches!(ret, ShcReturn::Requests(mut reqs) => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::ScheduleTimeout(Step::Prevote, ROUND_0)));
        assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Precommit && v.round == ROUND_0);
        assert!(reqs.is_empty());
    });
    // receiving Nil precommit requests and then decision on new round; just assert no panic and
    // decisions arrive after quorum.
    let _ = shc.handle_vote(&leader_fn, precommit(None, HEIGHT_0, ROUND_0, *VALIDATOR_ID_1));
    let ret = shc.handle_vote(&leader_fn, precommit(None, HEIGHT_0, ROUND_0, *VALIDATOR_ID_2));
    // assert that ret is ScheduleTimeoutPrecommit
    assert_matches!(ret, ShcReturn::Requests(mut reqs) => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::ScheduleTimeout(Step::Precommit, ROUND_0)));
        assert!(reqs.is_empty());
    });
    // No precommit quorum is reached. On TimeoutPrecommit(0) the proposer advances to round 1 with
    // a valid value (valid_round = Some(0)) and reproposes the same block, then broadcasts a
    // new prevote for round 1.
    let ret = shc.handle_event(&leader_fn, StateMachineEvent::TimeoutPrecommit(ROUND_0));
    // Expect Repropose with init.round == 1, init.valid_round == Some(0), and a
    // BroadcastVote(Prevote) for round 1.
    assert_matches!(ret, ShcReturn::Requests(mut reqs) => {
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
    assert_matches!(ret, ShcReturn::Requests(mut reqs) => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::StartBuildProposal(ROUND_0)));
        assert!(reqs.is_empty());
    });

    // Receive enough identical prevotes during awaiting_finished_building to trigger Timeout
    // (if they weren't duplicates)
    let duplicate_vote = prevote(Some(BLOCK.id.0), HEIGHT_0, ROUND_0, *VALIDATOR_ID_1);

    // First vote gets queued
    assert_matches!(
        shc.handle_vote(&leader_fn, duplicate_vote.clone()),
        ShcReturn::Requests(reqs) if reqs.is_empty()
    );

    // Remaining votes are duplicates - should be ignored
    for _ in 1..VALIDATORS.len() {
        assert_matches!(
            shc.handle_vote(&leader_fn, duplicate_vote.clone()),
            ShcReturn::Requests(reqs) if reqs.is_empty()
        );
    }

    // Finish building - processes the queue
    // Only one vote was queued (duplicates were ignored), so no TimeoutPrevote should be triggered,
    // only a broadcast vote
    let ret =
        shc.handle_event(&leader_fn, StateMachineEvent::FinishedBuilding(Some(BLOCK.id), ROUND_0));
    assert_matches!(ret, ShcReturn::Requests(mut reqs) => {
        assert_matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Prevote && v.round == ROUND_0);
        assert!(reqs.is_empty());
    });
}
