use futures::channel::{mpsc, oneshot};
use lazy_static::lazy_static;
use papyrus_protobuf::consensus::ConsensusMessage;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_types_core::felt::Felt;
use test_case::test_case;
use tokio;

use super::SingleHeightConsensus;
use crate::config::TimeoutsConfig;
use crate::single_height_consensus::{ShcReturn, ShcTask};
use crate::state_machine::StateMachineEvent;
use crate::test_utils::{precommit, prevote, MockTestContext, TestBlock};
use crate::types::{ConsensusError, ProposalInit, ValidatorId};

lazy_static! {
    static ref PROPOSER_ID: ValidatorId = 0_u32.into();
    static ref VALIDATOR_ID_1: ValidatorId = 1_u32.into();
    static ref VALIDATOR_ID_2: ValidatorId = 2_u32.into();
    static ref VALIDATOR_ID_3: ValidatorId = 3_u32.into();
    static ref VALIDATORS: Vec<ValidatorId> =
        vec![*PROPOSER_ID, *VALIDATOR_ID_1, *VALIDATOR_ID_2, *VALIDATOR_ID_3];
    static ref BLOCK: TestBlock = TestBlock { content: vec![1, 2, 3], id: BlockHash(Felt::ONE) };
    static ref PROPOSAL_INIT: ProposalInit = ProposalInit {
        height: BlockNumber(0),
        round: 0,
        proposer: *PROPOSER_ID,
        valid_round: None
    };
    static ref TIMEOUTS: TimeoutsConfig = TimeoutsConfig::default();
}

fn prevote_task(block_felt: Option<Felt>, round: u32) -> ShcTask {
    ShcTask {
        duration: TIMEOUTS.prevote_timeout,
        event: StateMachineEvent::Prevote(block_felt.map(BlockHash), round),
    }
}

fn precommit_task(block_felt: Option<Felt>, round: u32) -> ShcTask {
    ShcTask {
        duration: TIMEOUTS.precommit_timeout,
        event: StateMachineEvent::Precommit(block_felt.map(BlockHash), round),
    }
}

fn timeout_prevote_task(round: u32) -> ShcTask {
    ShcTask { duration: TIMEOUTS.prevote_timeout, event: StateMachineEvent::TimeoutPrevote(round) }
}

fn timeout_precommit_task(round: u32) -> ShcTask {
    ShcTask {
        duration: TIMEOUTS.precommit_timeout,
        event: StateMachineEvent::TimeoutPrecommit(round),
    }
}

#[tokio::test]
async fn proposer() {
    let mut context = MockTestContext::new();

    let mut shc = SingleHeightConsensus::new(
        BlockNumber(0),
        *PROPOSER_ID,
        VALIDATORS.to_vec(),
        TIMEOUTS.clone(),
    );

    context.expect_proposer().times(1).returning(move |_, _| *PROPOSER_ID);
    context.expect_build_proposal().times(1).returning(move |_, _| {
        let (block_sender, block_receiver) = oneshot::channel();
        block_sender.send(BLOCK.id).unwrap();
        block_receiver
    });
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &ConsensusMessage| msg == &prevote(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID))
        .returning(move |_| Ok(()));
    // Sends proposal and prevote.
    assert_eq!(
        shc.start(&mut context).await,
        Ok(ShcReturn::Tasks(vec![prevote_task(Some(BLOCK.id.0), 0)]))
    );
    assert_eq!(
        shc.handle_message(&mut context, prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_1)).await,
        Ok(ShcReturn::Tasks(Vec::new()))
    );
    // 3 of 4 Prevotes is enough to send a Precommit.
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &ConsensusMessage| {
            msg == &precommit(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID)
        })
        .returning(move |_| Ok(()));
    // The Node got a Prevote quorum.
    assert_eq!(
        shc.handle_message(&mut context, prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_2)).await,
        Ok(ShcReturn::Tasks(vec![timeout_prevote_task(0), precommit_task(Some(BLOCK.id.0), 0),]))
    );

    let precommits = vec![
        precommit(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_1),
        precommit(Some(Felt::TWO), 0, 0, *VALIDATOR_ID_3),
        precommit(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_2),
        precommit(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID),
    ];
    assert_eq!(
        shc.handle_message(&mut context, precommits[0].clone()).await,
        Ok(ShcReturn::Tasks(Vec::new()))
    );
    // The disagreeing vote counts towards the timeout, which uses a heterogeneous quorum, but not
    // the decision, which uses a homogenous quorum.
    assert_eq!(
        shc.handle_message(&mut context, precommits[1].clone()).await,
        Ok(ShcReturn::Tasks(vec![timeout_precommit_task(0),]))
    );
    let ShcReturn::Decision(decision) =
        shc.handle_message(&mut context, precommits[2].clone()).await.unwrap()
    else {
        panic!("Expected decision");
    };
    assert_eq!(decision.block, BLOCK.id);
    assert!(
        decision
            .precommits
            .into_iter()
            .all(|item| precommits.contains(&ConsensusMessage::Vote(item)))
    );
}

#[test_case(false; "single_proposal")]
#[test_case(true; "repeat_proposal")]
#[tokio::test]
async fn validator(repeat_proposal: bool) {
    let mut context = MockTestContext::new();

    // Creation calls to `context.validators`.
    let mut shc = SingleHeightConsensus::new(
        BlockNumber(0),
        *VALIDATOR_ID_1,
        VALIDATORS.to_vec(),
        TIMEOUTS.clone(),
    );

    // Send the proposal from the peer.
    let (fin_sender, fin_receiver) = oneshot::channel();
    fin_sender.send(BLOCK.id).unwrap();

    context.expect_proposer().returning(move |_, _| *PROPOSER_ID);
    context.expect_validate_proposal().times(1).returning(move |_, _, _| {
        let (block_sender, block_receiver) = oneshot::channel();
        block_sender.send(BLOCK.id).unwrap();
        block_receiver
    });
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &ConsensusMessage| {
            msg == &prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_1)
        })
        .returning(move |_| Ok(()));
    let res = shc
        .handle_proposal(
            &mut context,
            PROPOSAL_INIT.clone(),
            mpsc::channel(1).1, // content - ignored by SHC.
            fin_receiver,
        )
        .await;
    assert_eq!(res, Ok(ShcReturn::Tasks(vec![prevote_task(Some(BLOCK.id.0), 0),])));
    if repeat_proposal {
        // Send the same proposal again, which should be ignored (no expectations).
        let (fin_sender, fin_receiver) = oneshot::channel();
        fin_sender.send(BLOCK.id).unwrap();

        let res = shc
            .handle_proposal(
                &mut context,
                PROPOSAL_INIT.clone(),
                mpsc::channel(1).1, // content - ignored by SHC.
                fin_receiver,
            )
            .await;
        assert_eq!(res, Ok(ShcReturn::Tasks(Vec::new())));
    }
    assert_eq!(
        shc.handle_message(&mut context, prevote(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID)).await,
        Ok(ShcReturn::Tasks(Vec::new()))
    );
    // 3 of 4 Prevotes is enough to send a Precommit.
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &ConsensusMessage| {
            msg == &precommit(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_1)
        })
        .returning(move |_| Ok(()));
    // The Node got a Prevote quorum.
    assert_eq!(
        shc.handle_message(&mut context, prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_2)).await,
        Ok(ShcReturn::Tasks(vec![timeout_prevote_task(0), precommit_task(Some(BLOCK.id.0), 0)]))
    );

    let precommits = vec![
        precommit(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID),
        precommit(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_2),
        precommit(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_1),
    ];
    assert_eq!(
        shc.handle_message(&mut context, precommits[0].clone()).await,
        Ok(ShcReturn::Tasks(Vec::new()))
    );
    let ShcReturn::Decision(decision) =
        shc.handle_message(&mut context, precommits[1].clone()).await.unwrap()
    else {
        panic!("Expected decision");
    };
    assert_eq!(decision.block, BLOCK.id);
    assert!(
        decision
            .precommits
            .into_iter()
            .all(|item| precommits.contains(&ConsensusMessage::Vote(item)))
    );
}

#[test_case(true; "repeat")]
#[test_case(false; "equivocation")]
#[tokio::test]
async fn vote_twice(same_vote: bool) {
    let mut context = MockTestContext::new();

    let mut shc = SingleHeightConsensus::new(
        BlockNumber(0),
        *VALIDATOR_ID_1,
        VALIDATORS.to_vec(),
        TIMEOUTS.clone(),
    );

    let (fin_sender, fin_receiver) = oneshot::channel();
    fin_sender.send(BLOCK.id).unwrap();

    context.expect_proposer().times(1).returning(move |_, _| *PROPOSER_ID);
    context.expect_validate_proposal().times(1).returning(move |_, _, _| {
        let (block_sender, block_receiver) = oneshot::channel();
        block_sender.send(BLOCK.id).unwrap();
        block_receiver
    });
    context
        .expect_broadcast()
        .times(1) // Shows the repeat vote is ignored.
        .withf(move |msg: &ConsensusMessage| msg == &prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_1))
        .returning(move |_| Ok(()));
    let res = shc
        .handle_proposal(
            &mut context,
            PROPOSAL_INIT.clone(),
            mpsc::channel(1).1, // content - ignored by SHC.
            fin_receiver,
        )
        .await;
    assert_eq!(res, Ok(ShcReturn::Tasks(vec![prevote_task(Some(BLOCK.id.0), 0),])));

    let res = shc.handle_message(&mut context, prevote(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID)).await;
    assert_eq!(res, Ok(ShcReturn::Tasks(Vec::new())));

    context
    .expect_broadcast()
    .times(1) // Shows the repeat vote is ignored.
    .withf(move |msg: &ConsensusMessage| msg == &precommit(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_1))
    .returning(move |_| Ok(()));
    let res =
        shc.handle_message(&mut context, prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_2)).await;
    // The Node got a Prevote quorum.
    assert_eq!(
        res,
        Ok(ShcReturn::Tasks(vec![timeout_prevote_task(0), precommit_task(Some(BLOCK.id.0), 0),]))
    );

    let first_vote = precommit(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID);
    let res = shc.handle_message(&mut context, first_vote.clone()).await;
    assert_eq!(res, Ok(ShcReturn::Tasks(Vec::new())));

    let second_vote =
        if same_vote { first_vote.clone() } else { precommit(Some(Felt::TWO), 0, 0, *PROPOSER_ID) };
    let res = shc.handle_message(&mut context, second_vote.clone()).await;
    if same_vote {
        assert_eq!(res, Ok(ShcReturn::Tasks(Vec::new())));
    } else {
        assert!(matches!(res, Err(ConsensusError::Equivocation(_, _, _))));
    }

    let ShcReturn::Decision(decision) = shc
        .handle_message(&mut context, precommit(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_2))
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
        *PROPOSER_ID,
        VALIDATORS.to_vec(),
        TIMEOUTS.clone(),
    );

    context.expect_proposer().times(1).returning(move |_, _| *PROPOSER_ID);
    context.expect_build_proposal().times(1).returning(move |_, _| {
        let (block_sender, block_receiver) = oneshot::channel();
        block_sender.send(BLOCK.id).unwrap();
        block_receiver
    });
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &ConsensusMessage| msg == &prevote(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID))
        .returning(move |_| Ok(()));
    // Sends proposal and prevote.
    assert_eq!(
        shc.start(&mut context).await,
        Ok(ShcReturn::Tasks(vec![prevote_task(Some(BLOCK.id.0), 0),]))
    );
    assert_eq!(
        shc.handle_message(&mut context, prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_1)).await,
        Ok(ShcReturn::Tasks(Vec::new()))
    );
    // 3 of 4 Prevotes is enough to send a Precommit.
    context
        .expect_broadcast()
        .times(2) // vote rebroadcast
        .withf(move |msg: &ConsensusMessage| {
            msg == &precommit(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID)
        })
        .returning(move |_| Ok(()));
    // The Node got a Prevote quorum.
    assert_eq!(
        shc.handle_message(&mut context, prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_2)).await,
        Ok(ShcReturn::Tasks(vec![timeout_prevote_task(0), precommit_task(Some(BLOCK.id.0), 0),]))
    );
    // Re-broadcast vote.
    assert_eq!(
        shc.handle_event(&mut context, StateMachineEvent::Precommit(Some(BLOCK.id), 0),).await,
        Ok(ShcReturn::Tasks(vec![precommit_task(Some(BLOCK.id.0), 0),]))
    );
}

#[tokio::test]
async fn repropose() {
    let mut context = MockTestContext::new();

    let mut shc = SingleHeightConsensus::new(
        BlockNumber(0),
        *PROPOSER_ID,
        VALIDATORS.to_vec(),
        TIMEOUTS.clone(),
    );

    context.expect_proposer().returning(move |_, _| *PROPOSER_ID);
    context.expect_build_proposal().times(1).returning(move |_, _| {
        let (block_sender, block_receiver) = oneshot::channel();
        block_sender.send(BLOCK.id).unwrap();
        block_receiver
    });
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &ConsensusMessage| msg == &prevote(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID))
        .returning(move |_| Ok(()));
    // Sends proposal and prevote.
    shc.start(&mut context).await.unwrap();
    shc.handle_message(&mut context, prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_1))
        .await
        .unwrap();
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &ConsensusMessage| {
            msg == &precommit(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID)
        })
        .returning(move |_| Ok(()));
    // The Node got a Prevote quorum, and set valid proposal.
    assert_eq!(
        shc.handle_message(&mut context, prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_2)).await,
        Ok(ShcReturn::Tasks(vec![timeout_prevote_task(0), precommit_task(Some(BLOCK.id.0), 0),]))
    );
    // Advance to the next round.
    let precommits = vec![
        precommit(None, 0, 0, *VALIDATOR_ID_1),
        precommit(None, 0, 0, *VALIDATOR_ID_2),
        precommit(None, 0, 0, *VALIDATOR_ID_3),
    ];
    shc.handle_message(&mut context, precommits[0].clone()).await.unwrap();
    shc.handle_message(&mut context, precommits[1].clone()).await.unwrap();
    // After NIL precommits, the proposer should re-propose.
    context.expect_repropose().returning(move |id, init| {
        assert_eq!(init.height, BlockNumber(0));
        assert_eq!(id, BLOCK.id);
    });
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &ConsensusMessage| msg == &prevote(Some(BLOCK.id.0), 0, 1, *PROPOSER_ID))
        .returning(move |_| Ok(()));
    shc.handle_message(&mut context, precommits[2].clone()).await.unwrap();
    shc.handle_event(&mut context, StateMachineEvent::TimeoutPrecommit(0)).await.unwrap();

    let precommits = vec![
        precommit(Some(BLOCK.id.0), 0, 1, *VALIDATOR_ID_1),
        precommit(Some(BLOCK.id.0), 0, 1, *VALIDATOR_ID_2),
        precommit(Some(BLOCK.id.0), 0, 1, *VALIDATOR_ID_3),
    ];
    shc.handle_message(&mut context, precommits[0].clone()).await.unwrap();
    shc.handle_message(&mut context, precommits[1].clone()).await.unwrap();
    let ShcReturn::Decision(decision) =
        shc.handle_message(&mut context, precommits[2].clone()).await.unwrap()
    else {
        panic!("Expected decision");
    };
    assert_eq!(decision.block, BLOCK.id);
    assert!(
        decision
            .precommits
            .into_iter()
            .all(|item| precommits.contains(&ConsensusMessage::Vote(item)))
    );
}
