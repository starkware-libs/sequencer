use std::sync::{Arc, Mutex};
use std::time::Duration;

use apollo_consensus_config::config::{Timeout, TimeoutsConfig};
use apollo_protobuf::consensus::{ProposalFin, ProposalInit, Vote, DEFAULT_VALIDATOR_ID};
use futures::channel::{mpsc, oneshot};
use futures::SinkExt;
use lazy_static::lazy_static;
use mockall::predicate::eq;
use mockall::Sequence;
use starknet_api::block::BlockNumber;
use starknet_types_core::felt::Felt;
use test_case::test_case;

use super::SingleHeightConsensus;
use crate::single_height_consensus::{ShcReturn, ShcTask};
use crate::state_machine::StateMachineEvent;
use crate::storage::MockHeightVotedStorageTrait;
use crate::test_utils::{
    precommit,
    prevote,
    MockTestContext,
    NoOpHeightVotedStorage,
    TestBlock,
    TestProposalPart,
};
use crate::types::{ProposalCommitment, ValidatorId};
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
    static ref VALIDATE_PROPOSAL_EVENT: StateMachineEvent = StateMachineEvent::Proposal(
        Some(BLOCK.id),
        PROPOSAL_INIT.round,
        PROPOSAL_INIT.valid_round,
    );
    static ref PROPOSAL_FIN: ProposalFin = ProposalFin { proposal_commitment: BLOCK.id };
}

const CHANNEL_SIZE: usize = 1;

fn get_proposal_init_for_height(height: BlockNumber) -> ProposalInit {
    ProposalInit { height, ..*PROPOSAL_INIT }
}

fn prevote_task(block_felt: Option<Felt>, round: u32) -> ShcTask {
    let duration = TIMEOUTS.get_prevote_timeout(round);
    ShcTask::Prevote(
        duration,
        StateMachineEvent::Prevote(block_felt.map(ProposalCommitment), round),
    )
}

fn precommit_task(block_felt: Option<Felt>, round: u32) -> ShcTask {
    let duration = TIMEOUTS.get_precommit_timeout(round);
    ShcTask::Precommit(
        duration,
        StateMachineEvent::Precommit(block_felt.map(ProposalCommitment), round),
    )
}

fn timeout_prevote_task(round: u32) -> ShcTask {
    let duration = TIMEOUTS.get_prevote_timeout(round);
    ShcTask::TimeoutPrevote(duration, StateMachineEvent::TimeoutPrevote(round))
}

fn timeout_precommit_task(round: u32) -> ShcTask {
    let duration = TIMEOUTS.get_precommit_timeout(round);
    ShcTask::TimeoutPrecommit(duration, StateMachineEvent::TimeoutPrecommit(round))
}

async fn handle_proposal(
    height: BlockNumber,
    shc: &mut SingleHeightConsensus,
    context: &mut MockTestContext,
) -> ShcReturn {
    // Send the proposal from the peer.
    let (mut content_sender, content_receiver) = mpsc::channel(CHANNEL_SIZE);
    content_sender
        .send(TestProposalPart::Init(get_proposal_init_for_height(height)))
        .await
        .unwrap();

    shc.handle_proposal(context, get_proposal_init_for_height(height), content_receiver)
        .await
        .unwrap()
}

#[tokio::test]
async fn proposer() {
    let mut context = MockTestContext::new();

    let mut shc = SingleHeightConsensus::new(
        BlockNumber(0),
        false,
        *PROPOSER_ID,
        VALIDATORS.to_vec(),
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
        Arc::new(Mutex::new(NoOpHeightVotedStorage)),
    );

    context.expect_proposer().times(1).returning(move |_, _| *PROPOSER_ID);
    context.expect_build_proposal().times(1).returning(move |_, _| {
        let (block_sender, block_receiver) = oneshot::channel();
        block_sender.send(BLOCK.id).unwrap();
        block_receiver
    });
    context.expect_set_height_and_round().returning(move |_, _| ());
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &Vote| msg == &prevote(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID))
        .returning(move |_| Ok(()));
    // Sends proposal and prevote.
    let shc_ret = shc.start(&mut context).await.unwrap();
    assert_eq!(*shc_ret.as_tasks().unwrap()[0].as_build_proposal().unwrap().0, 0);
    assert_eq!(
        shc.handle_event(&mut context, StateMachineEvent::GetProposal(Some(BLOCK.id), 0)).await,
        Ok(ShcReturn::Tasks(vec![prevote_task(Some(BLOCK.id.0), 0)]))
    );
    assert_eq!(
        shc.handle_vote(&mut context, prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_1)).await,
        Ok(ShcReturn::Tasks(Vec::new()))
    );
    // 3 of 4 Prevotes is enough to send a Precommit.
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &Vote| msg == &precommit(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID))
        .returning(move |_| Ok(()));
    // The Node got a Prevote quorum.
    assert_eq!(
        shc.handle_vote(&mut context, prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_2)).await,
        Ok(ShcReturn::Tasks(vec![timeout_prevote_task(0), precommit_task(Some(BLOCK.id.0), 0),]))
    );

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
}

#[test_case(false; "single_proposal")]
#[test_case(true; "repeat_proposal")]
#[tokio::test]
async fn validator(repeat_proposal: bool) {
    const HEIGHT: BlockNumber = BlockNumber(0);
    let mut context = MockTestContext::new();

    // Creation calls to `context.validators`.
    let mut shc = SingleHeightConsensus::new(
        HEIGHT,
        false,
        *VALIDATOR_ID_1,
        VALIDATORS.to_vec(),
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
        Arc::new(Mutex::new(NoOpHeightVotedStorage)),
    );

    context.expect_proposer().returning(move |_, _| *PROPOSER_ID);
    context.expect_validate_proposal().times(1).returning(move |_, _, _| {
        let (block_sender, block_receiver) = oneshot::channel();
        block_sender.send(BLOCK.id).unwrap();
        block_receiver
    });
    context.expect_set_height_and_round().returning(move |_, _| ());
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &Vote| msg == &prevote(Some(BLOCK.id.0), HEIGHT.0, 0, *VALIDATOR_ID_1))
        .returning(move |_| Ok(()));
    let shc_ret = handle_proposal(HEIGHT, &mut shc, &mut context).await;
    assert_eq!(
        shc_ret.as_tasks().unwrap()[0].as_validate_proposal().unwrap().0,
        &get_proposal_init_for_height(HEIGHT)
    );
    assert_eq!(
        shc.handle_event(&mut context, VALIDATE_PROPOSAL_EVENT.clone()).await,
        Ok(ShcReturn::Tasks(vec![prevote_task(Some(BLOCK.id.0), 0)]))
    );
    if repeat_proposal {
        // Send the same proposal again, which should be ignored (no expectations).
        let shc_ret = handle_proposal(HEIGHT, &mut shc, &mut context).await;
        assert_eq!(shc_ret, ShcReturn::Tasks(Vec::new()));
    }
    assert_eq!(
        shc.handle_vote(&mut context, prevote(Some(BLOCK.id.0), HEIGHT.0, 0, *PROPOSER_ID)).await,
        Ok(ShcReturn::Tasks(Vec::new()))
    );
    // 3 of 4 Prevotes is enough to send a Precommit.
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &Vote| msg == &precommit(Some(BLOCK.id.0), HEIGHT.0, 0, *VALIDATOR_ID_1))
        .returning(move |_| Ok(()));
    // The Node got a Prevote quorum.
    assert_eq!(
        shc.handle_vote(&mut context, prevote(Some(BLOCK.id.0), HEIGHT.0, 0, *VALIDATOR_ID_2))
            .await,
        Ok(ShcReturn::Tasks(vec![timeout_prevote_task(0), precommit_task(Some(BLOCK.id.0), 0)]))
    );

    let precommits = vec![
        precommit(Some(BLOCK.id.0), HEIGHT.0, 0, *PROPOSER_ID),
        precommit(Some(BLOCK.id.0), HEIGHT.0, 0, *VALIDATOR_ID_2),
        precommit(Some(BLOCK.id.0), HEIGHT.0, 0, *VALIDATOR_ID_1),
    ];
    assert_eq!(
        shc.handle_vote(&mut context, precommits[0].clone()).await,
        Ok(ShcReturn::Tasks(Vec::new()))
    );
    let ShcReturn::Decision(decision) =
        shc.handle_vote(&mut context, precommits[1].clone()).await.unwrap()
    else {
        panic!("Expected decision");
    };
    assert_eq!(decision.block, BLOCK.id);
    assert!(decision.precommits.into_iter().all(|item| precommits.contains(&item)));
}

#[test_case(true; "repeat")]
#[test_case(false; "equivocation")]
#[tokio::test]
async fn vote_twice(same_vote: bool) {
    const HEIGHT: BlockNumber = BlockNumber(0);

    let mut context = MockTestContext::new();

    let mut shc = SingleHeightConsensus::new(
        HEIGHT,
        false,
        *VALIDATOR_ID_1,
        VALIDATORS.to_vec(),
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
        Arc::new(Mutex::new(NoOpHeightVotedStorage)),
    );

    context.expect_proposer().times(1).returning(move |_, _| *PROPOSER_ID);
    context.expect_validate_proposal().times(1).returning(move |_, _, _| {
        let (block_sender, block_receiver) = oneshot::channel();
        block_sender.send(BLOCK.id).unwrap();
        block_receiver
    });
    context.expect_set_height_and_round().returning(move |_, _| ());
    context
        .expect_broadcast()
        .times(1) // Shows the repeat vote is ignored.
        .withf(move |msg: &Vote| msg == &prevote(Some(BLOCK.id.0), HEIGHT.0, 0, *VALIDATOR_ID_1))
        .returning(move |_| Ok(()));
    let shc_ret = handle_proposal(HEIGHT, &mut shc, &mut context).await;
    assert_eq!(
        shc_ret.as_tasks().unwrap()[0].as_validate_proposal().unwrap().0,
        &get_proposal_init_for_height(HEIGHT)
    );
    assert_eq!(
        shc.handle_event(&mut context, VALIDATE_PROPOSAL_EVENT.clone()).await,
        Ok(ShcReturn::Tasks(vec![prevote_task(Some(BLOCK.id.0), 0)]))
    );

    let res =
        shc.handle_vote(&mut context, prevote(Some(BLOCK.id.0), HEIGHT.0, 0, *PROPOSER_ID)).await;
    assert_eq!(res, Ok(ShcReturn::Tasks(Vec::new())));

    context
    .expect_broadcast()
    .times(1) // Shows the repeat vote is ignored.
    .withf(move |msg: &Vote| msg == &precommit(Some(BLOCK.id.0), HEIGHT.0, 0, *VALIDATOR_ID_1))
    .returning(move |_| Ok(()));
    let res = shc
        .handle_vote(&mut context, prevote(Some(BLOCK.id.0), HEIGHT.0, 0, *VALIDATOR_ID_2))
        .await;
    // The Node got a Prevote quorum.
    assert_eq!(
        res,
        Ok(ShcReturn::Tasks(vec![timeout_prevote_task(0), precommit_task(Some(BLOCK.id.0), 0),]))
    );

    let first_vote = precommit(Some(BLOCK.id.0), HEIGHT.0, 0, *PROPOSER_ID);
    let res = shc.handle_vote(&mut context, first_vote.clone()).await;
    assert_eq!(res, Ok(ShcReturn::Tasks(Vec::new())));

    let second_vote = if same_vote {
        first_vote.clone()
    } else {
        precommit(Some(Felt::TWO), HEIGHT.0, 0, *PROPOSER_ID)
    };
    let res = shc.handle_vote(&mut context, second_vote.clone()).await;
    assert_eq!(res, Ok(ShcReturn::Tasks(Vec::new())));

    let ShcReturn::Decision(decision) = shc
        .handle_vote(&mut context, precommit(Some(BLOCK.id.0), HEIGHT.0, 0, *VALIDATOR_ID_2))
        .await
        .unwrap()
    else {
        panic!("Expected decision");
    };
    assert_eq!(decision.block, BLOCK.id);
}

#[tokio::test]
async fn rebroadcast_votes() {
    let mut context = MockTestContext::new();

    let mut shc = SingleHeightConsensus::new(
        BlockNumber(0),
        false,
        *PROPOSER_ID,
        VALIDATORS.to_vec(),
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
        Arc::new(Mutex::new(NoOpHeightVotedStorage)),
    );

    context.expect_proposer().times(1).returning(move |_, _| *PROPOSER_ID);
    context.expect_build_proposal().times(1).returning(move |_, _| {
        let (block_sender, block_receiver) = oneshot::channel();
        block_sender.send(BLOCK.id).unwrap();
        block_receiver
    });
    context.expect_set_height_and_round().returning(move |_, _| ());
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &Vote| msg == &prevote(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID))
        .returning(move |_| Ok(()));
    // Sends proposal and prevote.
    let shc_ret = shc.start(&mut context).await.unwrap();
    assert_eq!(*shc_ret.as_tasks().unwrap()[0].as_build_proposal().unwrap().0, 0);
    assert_eq!(
        shc.handle_event(&mut context, StateMachineEvent::GetProposal(Some(BLOCK.id), 0)).await,
        Ok(ShcReturn::Tasks(vec![prevote_task(Some(BLOCK.id.0), 0)]))
    );
    assert_eq!(
        shc.handle_vote(&mut context, prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_1)).await,
        Ok(ShcReturn::Tasks(Vec::new()))
    );
    // 3 of 4 Prevotes is enough to send a Precommit.
    context
        .expect_broadcast()
        .times(2) // vote rebroadcast
        .withf(move |msg: &Vote| {
            msg == &precommit(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID)
        })
        .returning(move |_| Ok(()));
    // The Node got a Prevote quorum.
    assert_eq!(
        shc.handle_vote(&mut context, prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_2)).await,
        Ok(ShcReturn::Tasks(vec![timeout_prevote_task(0), precommit_task(Some(BLOCK.id.0), 0),]))
    );
    // Re-broadcast vote.
    assert_eq!(
        shc.handle_event(&mut context, StateMachineEvent::Precommit(Some(BLOCK.id), 0)).await,
        Ok(ShcReturn::Tasks(vec![precommit_task(Some(BLOCK.id.0), 0),]))
    );
}

#[tokio::test]
async fn repropose() {
    let mut context = MockTestContext::new();

    let mut shc = SingleHeightConsensus::new(
        BlockNumber(0),
        false,
        *PROPOSER_ID,
        VALIDATORS.to_vec(),
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
        Arc::new(Mutex::new(NoOpHeightVotedStorage)),
    );

    context.expect_proposer().returning(move |_, _| *PROPOSER_ID);
    context.expect_build_proposal().times(1).returning(move |_, _| {
        let (block_sender, block_receiver) = oneshot::channel();
        block_sender.send(BLOCK.id).unwrap();
        block_receiver
    });
    context.expect_set_height_and_round().returning(move |_, _| ());
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &Vote| msg == &prevote(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID))
        .returning(move |_| Ok(()));
    // Sends proposal and prevote.
    shc.start(&mut context).await.unwrap();
    shc.handle_event(&mut context, StateMachineEvent::GetProposal(Some(BLOCK.id), 0))
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
        Ok(ShcReturn::Tasks(vec![timeout_prevote_task(0), precommit_task(Some(BLOCK.id.0), 0),]))
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
    });
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &Vote| msg == &prevote(Some(BLOCK.id.0), 0, 1, *PROPOSER_ID))
        .returning(move |_| Ok(()));
    shc.handle_vote(&mut context, precommits[2].clone()).await.unwrap();
    shc.handle_event(&mut context, StateMachineEvent::TimeoutPrecommit(0)).await.unwrap();

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
        Ok(ShcReturn::Tasks(vec![prevote_task(Some(BLOCK.id.0), 0)]))
    );

    // This is the call that will result in the prevote broadcast and storage write.
    let res =
        shc.handle_vote(&mut context, prevote(Some(BLOCK.id.0), HEIGHT.0, 0, *PROPOSER_ID)).await;
    assert_eq!(res, Ok(ShcReturn::Tasks(Vec::new())));

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
        Ok(ShcReturn::Tasks(vec![timeout_prevote_task(0), precommit_task(Some(BLOCK.id.0), 0)]))
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
    );
}

#[tokio::test]
async fn shc_applies_proposal_timeouts_across_rounds() {
    let mut context = MockTestContext::new();

    let timeouts = TimeoutsConfig::new(
        // Proposal timeouts: base=100ms, delta=60ms, max=200ms.
        Timeout::new(
            Duration::from_millis(100),
            Duration::from_millis(60),
            Duration::from_millis(200),
        ),
        // unused prevote and precommit timeouts.
        Timeout::new(
            Duration::from_millis(100),
            Duration::from_millis(10),
            Duration::from_millis(150),
        ),
        Timeout::new(
            Duration::from_millis(100),
            Duration::from_millis(10),
            Duration::from_millis(150),
        ),
    );

    let mut shc = SingleHeightConsensus::new(
        BlockNumber(0),
        false,
        *PROPOSER_ID,
        VALIDATORS.to_vec(),
        QuorumType::Byzantine,
        timeouts.clone(),
        Arc::new(Mutex::new(NoOpHeightVotedStorage)),
    );
    // context expectations.
    context.expect_proposer().returning(move |_, _| *PROPOSER_ID);
    context.expect_set_height_and_round().returning(move |_, _| ());

    // Round 0 validate: timeout = 100ms.
    let expected_validate_timeout_r0 = Duration::from_millis(100);
    context
        .expect_validate_proposal()
        .times(1)
        .withf(move |init, timeout, _| init.round == 0 && *timeout == expected_validate_timeout_r0)
        .returning(move |_, _, _| {
            let (tx, rx) = oneshot::channel();
            tx.send(BLOCK.id).ok();
            rx
        });

    let (mut content_tx, content_rx) = mpsc::channel(CHANNEL_SIZE);
    content_tx.send(TestProposalPart::Init(ProposalInit::default())).await.unwrap();
    let _ = shc.handle_proposal(&mut context, ProposalInit::default(), content_rx).await.unwrap();

    // Round 1 validate: timeout = 160ms.
    let expected_validate_timeout_r1 = Duration::from_millis(160);
    context
        .expect_validate_proposal()
        .times(1)
        .withf(move |init, timeout, _| init.round == 1 && *timeout == expected_validate_timeout_r1)
        .returning(move |_, _, _| {
            let (tx, rx) = oneshot::channel();
            tx.send(BLOCK.id).ok();
            rx
        });
    let (mut content_tx2, content_rx2) = mpsc::channel(CHANNEL_SIZE);
    let round1_init = ProposalInit {
        height: BlockNumber(0),
        round: 1,
        proposer: *PROPOSER_ID,
        ..Default::default()
    };
    content_tx2.send(TestProposalPart::Init(round1_init)).await.unwrap();
    let _ = shc.handle_proposal(&mut context, round1_init, content_rx2).await.unwrap();

    // Round 2 validate: timeout = min(100 + 2*60, 200) = 200ms (capped).
    let expected_validate_timeout_r2 = Duration::from_millis(200);
    context
        .expect_validate_proposal()
        .times(1)
        .withf(move |init, timeout, _| init.round == 2 && *timeout == expected_validate_timeout_r2)
        .returning(move |_, _, _| {
            let (tx, rx) = oneshot::channel();
            tx.send(BLOCK.id).ok();
            rx
        });
    let (mut content_tx3, content_rx3) = mpsc::channel(CHANNEL_SIZE);
    let round2_init = ProposalInit {
        height: BlockNumber(0),
        round: 2,
        proposer: *PROPOSER_ID,
        ..Default::default()
    };
    content_tx3.send(TestProposalPart::Init(round2_init)).await.unwrap();
    let _ = shc.handle_proposal(&mut context, round2_init, content_rx3).await.unwrap();
}
