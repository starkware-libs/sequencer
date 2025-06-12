use apollo_protobuf::consensus::{ProposalFin, ProposalInit, Vote, DEFAULT_VALIDATOR_ID};
use futures::channel::{mpsc, oneshot};
use futures::SinkExt;
use lazy_static::lazy_static;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_types_core::felt::Felt;
use test_case::test_case;

use super::SingleHeightConsensus;
use crate::config::TimeoutsConfig;
use crate::single_height_consensus::{ShcEvent, ShcReturn, ShcTask};
use crate::state_machine::StateMachineEvent;
use crate::test_utils::{precommit, prevote, MockTestContext, TestBlock, TestProposalPart};
use crate::types::ValidatorId;
use crate::votes_threshold::QuorumType;

lazy_static! {
    static ref PROPOSER_ID: ValidatorId = DEFAULT_VALIDATOR_ID.into();
    static ref VALIDATOR_ID_1: ValidatorId = (DEFAULT_VALIDATOR_ID + 1).into();
    static ref VALIDATOR_ID_2: ValidatorId = (DEFAULT_VALIDATOR_ID + 2).into();
    static ref VALIDATOR_ID_3: ValidatorId = (DEFAULT_VALIDATOR_ID + 3).into();
    static ref VALIDATORS: Vec<ValidatorId> =
        vec![*PROPOSER_ID, *VALIDATOR_ID_1, *VALIDATOR_ID_2, *VALIDATOR_ID_3];
    static ref BLOCK: TestBlock = TestBlock { content: vec![1, 2, 3], id: BlockHash(Felt::ONE) };
    static ref PROPOSAL_INIT: ProposalInit =
        ProposalInit { proposer: *PROPOSER_ID, ..Default::default() };
    static ref TIMEOUTS: TimeoutsConfig = TimeoutsConfig::default();
    static ref VALIDATE_PROPOSAL_EVENT: ShcEvent = ShcEvent::ValidateProposal(
        StateMachineEvent::Proposal(Some(BLOCK.id), PROPOSAL_INIT.round, PROPOSAL_INIT.valid_round,),
    );
    static ref PROPOSAL_FIN: ProposalFin = ProposalFin { proposal_commitment: BLOCK.id };
}

const CHANNEL_SIZE: usize = 1;

fn prevote_task(block_felt: Option<Felt>, round: u32) -> ShcTask {
    ShcTask::Prevote(
        TIMEOUTS.prevote_timeout,
        StateMachineEvent::Prevote(block_felt.map(BlockHash), round),
    )
}

fn precommit_task(block_felt: Option<Felt>, round: u32) -> ShcTask {
    ShcTask::Precommit(
        TIMEOUTS.precommit_timeout,
        StateMachineEvent::Precommit(block_felt.map(BlockHash), round),
    )
}

fn timeout_prevote_task(round: u32) -> ShcTask {
    ShcTask::TimeoutPrevote(TIMEOUTS.prevote_timeout, StateMachineEvent::TimeoutPrevote(round))
}

fn timeout_precommit_task(round: u32) -> ShcTask {
    ShcTask::TimeoutPrecommit(
        TIMEOUTS.precommit_timeout,
        StateMachineEvent::TimeoutPrecommit(round),
    )
}

async fn handle_proposal(
    shc: &mut SingleHeightConsensus,
    context: &mut MockTestContext,
) -> ShcReturn {
    // Send the proposal from the peer.
    let (mut content_sender, content_receiver) = mpsc::channel(CHANNEL_SIZE);
    content_sender.send(TestProposalPart::Init(ProposalInit::default())).await.unwrap();

    shc.handle_proposal(context, *PROPOSAL_INIT, content_receiver).await.unwrap()
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
        shc.handle_event(
            &mut context,
            ShcEvent::BuildProposal(StateMachineEvent::GetProposal(Some(BLOCK.id), 0)),
        )
        .await,
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
    let mut context = MockTestContext::new();

    // Creation calls to `context.validators`.
    let mut shc = SingleHeightConsensus::new(
        BlockNumber(0),
        false,
        *VALIDATOR_ID_1,
        VALIDATORS.to_vec(),
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
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
        .withf(move |msg: &Vote| msg == &prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_1))
        .returning(move |_| Ok(()));
    let shc_ret = handle_proposal(&mut shc, &mut context).await;
    assert_eq!(shc_ret.as_tasks().unwrap()[0].as_validate_proposal().unwrap().0, &*PROPOSAL_INIT);
    assert_eq!(
        shc.handle_event(&mut context, VALIDATE_PROPOSAL_EVENT.clone()).await,
        Ok(ShcReturn::Tasks(vec![prevote_task(Some(BLOCK.id.0), 0)]))
    );
    if repeat_proposal {
        // Send the same proposal again, which should be ignored (no expectations).
        let shc_ret = handle_proposal(&mut shc, &mut context).await;
        assert_eq!(shc_ret, ShcReturn::Tasks(Vec::new()));
    }
    assert_eq!(
        shc.handle_vote(&mut context, prevote(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID)).await,
        Ok(ShcReturn::Tasks(Vec::new()))
    );
    // 3 of 4 Prevotes is enough to send a Precommit.
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &Vote| msg == &precommit(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_1))
        .returning(move |_| Ok(()));
    // The Node got a Prevote quorum.
    assert_eq!(
        shc.handle_vote(&mut context, prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_2)).await,
        Ok(ShcReturn::Tasks(vec![timeout_prevote_task(0), precommit_task(Some(BLOCK.id.0), 0)]))
    );

    let precommits = vec![
        precommit(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID),
        precommit(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_2),
        precommit(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_1),
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
    let mut context = MockTestContext::new();

    let mut shc = SingleHeightConsensus::new(
        BlockNumber(0),
        false,
        *VALIDATOR_ID_1,
        VALIDATORS.to_vec(),
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
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
        .withf(move |msg: &Vote| msg == &prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_1))
        .returning(move |_| Ok(()));
    let shc_ret = handle_proposal(&mut shc, &mut context).await;
    assert_eq!(shc_ret.as_tasks().unwrap()[0].as_validate_proposal().unwrap().0, &*PROPOSAL_INIT,);
    assert_eq!(
        shc.handle_event(&mut context, VALIDATE_PROPOSAL_EVENT.clone()).await,
        Ok(ShcReturn::Tasks(vec![prevote_task(Some(BLOCK.id.0), 0)]))
    );

    let res = shc.handle_vote(&mut context, prevote(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID)).await;
    assert_eq!(res, Ok(ShcReturn::Tasks(Vec::new())));

    context
    .expect_broadcast()
    .times(1) // Shows the repeat vote is ignored.
    .withf(move |msg: &Vote| msg == &precommit(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_1))
    .returning(move |_| Ok(()));
    let res = shc.handle_vote(&mut context, prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_2)).await;
    // The Node got a Prevote quorum.
    assert_eq!(
        res,
        Ok(ShcReturn::Tasks(vec![timeout_prevote_task(0), precommit_task(Some(BLOCK.id.0), 0),]))
    );

    let first_vote = precommit(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID);
    let res = shc.handle_vote(&mut context, first_vote.clone()).await;
    assert_eq!(res, Ok(ShcReturn::Tasks(Vec::new())));

    let second_vote =
        if same_vote { first_vote.clone() } else { precommit(Some(Felt::TWO), 0, 0, *PROPOSER_ID) };
    let res = shc.handle_vote(&mut context, second_vote.clone()).await;
    assert_eq!(res, Ok(ShcReturn::Tasks(Vec::new())));

    let ShcReturn::Decision(decision) = shc
        .handle_vote(&mut context, precommit(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_2))
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
        shc.handle_event(
            &mut context,
            ShcEvent::BuildProposal(StateMachineEvent::GetProposal(Some(BLOCK.id), 0)),
        )
        .await,
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
        shc.handle_event(
            &mut context,
            ShcEvent::Precommit(StateMachineEvent::Precommit(Some(BLOCK.id), 0))
        )
        .await,
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
    shc.handle_event(
        &mut context,
        ShcEvent::BuildProposal(StateMachineEvent::GetProposal(Some(BLOCK.id), 0)),
    )
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
    shc.handle_event(
        &mut context,
        ShcEvent::TimeoutPrecommit(StateMachineEvent::TimeoutPrecommit(0)),
    )
    .await
    .unwrap();

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
