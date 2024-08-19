use std::sync::{Arc, OnceLock};

use futures::channel::{mpsc, oneshot};
use lazy_static::lazy_static;
use papyrus_protobuf::consensus::ConsensusMessage;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_types_core::felt::Felt;
use test_case::test_case;
use tokio;

use super::SingleHeightConsensus;
use crate::test_utils::{precommit, prevote, MockTestContext, TestBlock};
use crate::types::{ConsensusBlock, ConsensusError, ProposalInit, ValidatorId};

lazy_static! {
    static ref PROPOSER_ID: ValidatorId = 0_u32.into();
    static ref VALIDATOR_ID_1: ValidatorId = 1_u32.into();
    static ref VALIDATOR_ID_2: ValidatorId = 2_u32.into();
    static ref VALIDATOR_ID_3: ValidatorId = 3_u32.into();
    static ref VALIDATORS: Vec<ValidatorId> =
        vec![*PROPOSER_ID, *VALIDATOR_ID_1, *VALIDATOR_ID_2, *VALIDATOR_ID_3];
    static ref BLOCK: TestBlock = TestBlock { content: vec![1, 2, 3], id: BlockHash(Felt::ONE) };
    static ref BLOCK_ID: BlockHash = BLOCK.id();
    static ref PROPOSAL_INIT: ProposalInit =
        ProposalInit { height: BlockNumber(0), round: 0, proposer: *PROPOSER_ID };
}

#[tokio::test]
async fn proposer() {
    let mut context = MockTestContext::new();

    let mut shc = SingleHeightConsensus::new(BlockNumber(0), *VALIDATOR_ID_1, VALIDATORS.to_vec());

    context.expect_proposer().times(1).returning(move |_, _| *VALIDATOR_ID_1);
    context.expect_build_proposal().times(1).returning(move |_| {
        let (_, content_receiver) = mpsc::channel(1);
        let (block_sender, block_receiver) = oneshot::channel();
        block_sender.send(BLOCK.clone()).unwrap();
        (content_receiver, block_receiver)
    });
    let fin_receiver = Arc::new(OnceLock::new());
    let fin_receiver_clone = Arc::clone(&fin_receiver);
    context.expect_propose().times(1).return_once(move |init, _, fin_receiver| {
        // Ignore content receiver, since this is the context's responsibility.
        assert_eq!(init.height, BlockNumber(0));
        assert_eq!(init.proposer, *VALIDATOR_ID_1);
        fin_receiver_clone.set(fin_receiver).unwrap();
        Ok(())
    });
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &ConsensusMessage| {
            msg == &prevote(Some(*BLOCK_ID), 0, 0, *VALIDATOR_ID_1)
        })
        .returning(move |_| Ok(()));
    // Sends proposal and prevote.
    assert!(matches!(shc.start(&mut context).await, Ok(None)));

    assert_eq!(
        shc.handle_message(&mut context, prevote(Some(*BLOCK_ID), 0, 0, *PROPOSER_ID)).await,
        Ok(None)
    );
    // 3 of 4 Prevotes is enough to send a Precommit.
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &ConsensusMessage| {
            msg == &precommit(Some(*BLOCK_ID), 0, 0, *VALIDATOR_ID_1)
        })
        .returning(move |_| Ok(()));
    assert_eq!(
        shc.handle_message(&mut context, prevote(Some(*BLOCK_ID), 0, 0, *VALIDATOR_ID_2)).await,
        Ok(None)
    );

    let precommits = vec![
        precommit(Some(*BLOCK_ID), 0, 0, *VALIDATOR_ID_1),
        precommit(Some(BlockHash(Felt::TWO)), 0, 0, *VALIDATOR_ID_3), // Ignores since disagrees.
        precommit(Some(*BLOCK_ID), 0, 0, *PROPOSER_ID),
        precommit(Some(*BLOCK_ID), 0, 0, *VALIDATOR_ID_2),
    ];
    assert_eq!(shc.handle_message(&mut context, precommits[1].clone()).await, Ok(None));
    assert_eq!(shc.handle_message(&mut context, precommits[2].clone()).await, Ok(None));
    let decision = shc.handle_message(&mut context, precommits[3].clone()).await.unwrap().unwrap();
    assert_eq!(decision.block, *BLOCK);
    assert!(
        decision
            .precommits
            .into_iter()
            .all(|item| precommits.contains(&ConsensusMessage::Vote(item)))
    );

    // Check the fin sent to the network.
    let fin = Arc::into_inner(fin_receiver).unwrap().take().unwrap().await.unwrap();
    assert_eq!(fin, *BLOCK_ID);
}

#[test_case(false; "single_proposal")]
#[test_case(true; "repeat_proposal")]
#[tokio::test]
async fn validator(repeat_proposal: bool) {
    let mut context = MockTestContext::new();

    // Creation calls to `context.validators`.
    let mut shc = SingleHeightConsensus::new(BlockNumber(0), *VALIDATOR_ID_1, VALIDATORS.to_vec());

    // Send the proposal from the peer.
    let (fin_sender, fin_receiver) = oneshot::channel();
    fin_sender.send(*BLOCK_ID).unwrap();

    context.expect_proposer().returning(move |_, _| *PROPOSER_ID);
    context.expect_validate_proposal().times(1).returning(move |_, _| {
        let (block_sender, block_receiver) = oneshot::channel();
        block_sender.send(BLOCK.clone()).unwrap();
        block_receiver
    });
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &ConsensusMessage| {
            msg == &prevote(Some(*BLOCK_ID), 0, 0, *VALIDATOR_ID_1)
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
    assert_eq!(res, Ok(None));
    if repeat_proposal {
        // Send the same proposal again, which should be ignored (no expectations).
        let (fin_sender, fin_receiver) = oneshot::channel();
        fin_sender.send(*BLOCK_ID).unwrap();

        let res = shc
            .handle_proposal(
                &mut context,
                PROPOSAL_INIT.clone(),
                mpsc::channel(1).1, // content - ignored by SHC.
                fin_receiver,
            )
            .await;
        assert_eq!(res, Ok(None));
    }
    assert_eq!(
        shc.handle_message(&mut context, prevote(Some(*BLOCK_ID), 0, 0, *PROPOSER_ID)).await,
        Ok(None)
    );
    // 3 of 4 Prevotes is enough to send a Precommit.
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &ConsensusMessage| {
            msg == &precommit(Some(*BLOCK_ID), 0, 0, *VALIDATOR_ID_1)
        })
        .returning(move |_| Ok(()));
    assert_eq!(
        shc.handle_message(&mut context, prevote(Some(*BLOCK_ID), 0, 0, *VALIDATOR_ID_2)).await,
        Ok(None)
    );

    let precommits = vec![
        precommit(Some(*BLOCK_ID), 0, 0, *PROPOSER_ID),
        precommit(Some(*BLOCK_ID), 0, 0, *VALIDATOR_ID_2),
        precommit(Some(*BLOCK_ID), 0, 0, *VALIDATOR_ID_1),
    ];
    assert_eq!(shc.handle_message(&mut context, precommits[0].clone()).await, Ok(None));
    let decision = shc.handle_message(&mut context, precommits[1].clone()).await.unwrap().unwrap();
    assert_eq!(decision.block, *BLOCK);
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

    let mut shc = SingleHeightConsensus::new(BlockNumber(0), *VALIDATOR_ID_1, VALIDATORS.to_vec());

    let (fin_sender, fin_receiver) = oneshot::channel();
    fin_sender.send(*BLOCK_ID).unwrap();

    context.expect_proposer().times(1).returning(move |_, _| *PROPOSER_ID);
    context.expect_validate_proposal().times(1).returning(move |_, _| {
        let (block_sender, block_receiver) = oneshot::channel();
        block_sender.send(BLOCK.clone()).unwrap();
        block_receiver
    });
    context
        .expect_broadcast()
        .times(1) // Shows the repeat vote is ignored.
        .withf(move |msg: &ConsensusMessage| msg == &prevote(Some(*BLOCK_ID), 0, 0, *VALIDATOR_ID_1))
        .returning(move |_| Ok(()));
    let res = shc
        .handle_proposal(
            &mut context,
            PROPOSAL_INIT.clone(),
            mpsc::channel(1).1, // content - ignored by SHC.
            fin_receiver,
        )
        .await;
    assert_eq!(res, Ok(None));

    let res = shc.handle_message(&mut context, prevote(Some(*BLOCK_ID), 0, 0, *PROPOSER_ID)).await;
    assert_eq!(res, Ok(None));

    context
    .expect_broadcast()
    .times(1) // Shows the repeat vote is ignored.
    .withf(move |msg: &ConsensusMessage| msg == &precommit(Some(*BLOCK_ID), 0, 0, *VALIDATOR_ID_1))
    .returning(move |_| Ok(()));
    let res =
        shc.handle_message(&mut context, prevote(Some(*BLOCK_ID), 0, 0, *VALIDATOR_ID_2)).await;
    assert_eq!(res, Ok(None));

    let first_vote = precommit(Some(*BLOCK_ID), 0, 0, *PROPOSER_ID);
    let res = shc.handle_message(&mut context, first_vote.clone()).await;
    assert_eq!(res, Ok(None));

    let second_vote = if same_vote {
        first_vote.clone()
    } else {
        precommit(Some(BlockHash(Felt::TWO)), 0, 0, *PROPOSER_ID)
    };
    let res = shc.handle_message(&mut context, second_vote.clone()).await;
    if same_vote {
        assert_eq!(res, Ok(None));
    } else {
        assert!(matches!(res, Err(ConsensusError::Equivocation(_, _, _))));
    }

    let decision = shc
        .handle_message(&mut context, precommit(Some(*BLOCK_ID), 0, 0, *VALIDATOR_ID_2))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(decision.block, *BLOCK);
}
