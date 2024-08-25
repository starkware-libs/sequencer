use std::time::Duration;
use std::vec;

use async_trait::async_trait;
use futures::channel::{mpsc, oneshot};
use futures::SinkExt;
use mockall::mock;
use mockall::predicate::eq;
use papyrus_network::network_manager::ReportSender;
use papyrus_protobuf::consensus::{ConsensusMessage, Vote};
use papyrus_protobuf::converters::ProtobufConversionError;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::transaction::Transaction;
use starknet_types_core::felt::Felt;

use super::{run_consensus, MultiHeightManager};
use crate::test_utils::{
    precommit,
    prevote,
    proposal,
    PROPOSER_ID,
    TIMEOUTS,
    VALIDATOR_ID_1,
    VALIDATOR_ID_2,
    VALIDATOR_ID_3,
};
use crate::types::{
    ConsensusBlock,
    ConsensusContext,
    ConsensusError,
    ProposalInit,
    Round,
    ValidatorId,
};

// TODO(matan): Switch to using TestBlock & MockTestContext in `test_utils` once streaming is
// supported. Streaming should allow us to make the Manager generic over the content.
#[derive(Debug, PartialEq, Clone)]
pub struct TestBlock {
    pub content: Vec<Transaction>,
    pub id: BlockHash,
}

impl ConsensusBlock for TestBlock {
    type ProposalChunk = Transaction;
    type ProposalIter = std::vec::IntoIter<Transaction>;

    fn id(&self) -> BlockHash {
        self.id
    }

    fn proposal_iter(&self) -> Self::ProposalIter {
        self.content.clone().into_iter()
    }
}

mock! {
    pub TestContext {}

    #[async_trait]
    impl ConsensusContext for TestContext {
        type Block = TestBlock;

        async fn build_proposal(&self, height: BlockNumber) -> (
            mpsc::Receiver<Transaction>,
            oneshot::Receiver<TestBlock>
        );

        async fn validate_proposal(
            &self,
            height: BlockNumber,
            content: mpsc::Receiver<Transaction>
        ) -> oneshot::Receiver<TestBlock>;

        async fn validators(&self, height: BlockNumber) -> Vec<ValidatorId>;

        fn proposer(&self, height: BlockNumber, round: Round) -> ValidatorId;

        async fn broadcast(&mut self, message: ConsensusMessage) -> Result<(), ConsensusError>;

        async fn propose(
            &self,
            init: ProposalInit,
            content_receiver: mpsc::Receiver<Transaction>,
            fin_receiver: oneshot::Receiver<BlockHash>,
        ) -> Result<(), ConsensusError>;

        async fn decision_reached(
            &mut self,
            block: TestBlock,
            precommits: Vec<Vote>,
        ) -> Result<(), ConsensusError>;
    }
}

type Sender =
    mpsc::UnboundedSender<(Result<ConsensusMessage, ProtobufConversionError>, ReportSender)>;

async fn send(sender: &mut Sender, msg: ConsensusMessage) {
    sender
        .send((Ok(msg.clone()), oneshot::channel().0))
        .await
        .unwrap_or_else(|_| panic!("Failed to send message: {msg:?}"));
}

#[tokio::test]
async fn manager_multiple_heights_unordered() {
    let (mut sender, mut receiver) = mpsc::unbounded();
    // Send messages for height 2 followed by those for height 1.
    send(&mut sender, proposal(Felt::TWO, 2, 0, *PROPOSER_ID)).await;
    send(&mut sender, prevote(Some(Felt::TWO), 2, 0, *PROPOSER_ID)).await;
    send(&mut sender, precommit(Some(Felt::TWO), 2, 0, *PROPOSER_ID)).await;
    send(&mut sender, proposal(Felt::ONE, 1, 0, *PROPOSER_ID)).await;
    send(&mut sender, prevote(Some(Felt::ONE), 1, 0, *PROPOSER_ID)).await;
    send(&mut sender, precommit(Some(Felt::ONE), 1, 0, *PROPOSER_ID)).await;

    let mut context = MockTestContext::new();
    // Run the manager for height 1.
    context
        .expect_validate_proposal()
        .return_once(move |_, _| {
            let (block_sender, block_receiver) = oneshot::channel();
            block_sender.send(TestBlock { content: vec![], id: BlockHash(Felt::ONE) }).unwrap();
            block_receiver
        })
        .times(1);
    context.expect_validators().returning(move |_| vec![*PROPOSER_ID, *VALIDATOR_ID_1]);
    context.expect_proposer().returning(move |_, _| *PROPOSER_ID);
    context.expect_broadcast().returning(move |_| Ok(()));

    let mut manager = MultiHeightManager::new(*VALIDATOR_ID_1, TIMEOUTS.clone());
    let decision = manager.run_height(&mut context, BlockNumber(1), &mut receiver).await.unwrap();
    assert_eq!(decision.block.id(), BlockHash(Felt::ONE));

    // Run the manager for height 2.
    context
        .expect_validate_proposal()
        .return_once(move |_, _| {
            let (block_sender, block_receiver) = oneshot::channel();
            block_sender.send(TestBlock { content: vec![], id: BlockHash(Felt::TWO) }).unwrap();
            block_receiver
        })
        .times(1);
    let decision = manager.run_height(&mut context, BlockNumber(2), &mut receiver).await.unwrap();
    assert_eq!(decision.block.id(), BlockHash(Felt::TWO));
}

#[tokio::test]
async fn run_consensus_sync() {
    // Set expectations.
    let mut context = MockTestContext::new();
    let (decision_tx, decision_rx) = oneshot::channel();

    context.expect_validate_proposal().return_once(move |_, _| {
        let (block_sender, block_receiver) = oneshot::channel();
        block_sender.send(TestBlock { content: vec![], id: BlockHash(Felt::TWO) }).unwrap();
        block_receiver
    });
    context.expect_validators().returning(move |_| vec![*PROPOSER_ID, *VALIDATOR_ID_1]);
    context.expect_proposer().returning(move |_, _| *PROPOSER_ID);
    context.expect_broadcast().returning(move |_| Ok(()));
    context.expect_decision_reached().return_once(move |block, votes| {
        assert_eq!(block.id(), BlockHash(Felt::TWO));
        assert_eq!(votes[0].height, 2);
        decision_tx.send(()).unwrap();
        Ok(())
    });

    // Send messages for height 2.
    let (mut network_sender, mut network_receiver) = mpsc::unbounded();
    send(&mut network_sender, proposal(Felt::TWO, 2, 0, *PROPOSER_ID)).await;
    send(&mut network_sender, prevote(Some(Felt::TWO), 2, 0, *PROPOSER_ID)).await;
    send(&mut network_sender, precommit(Some(Felt::TWO), 2, 0, *PROPOSER_ID)).await;

    // Start at height 1.
    let (mut sync_sender, mut sync_receiver) = mpsc::unbounded();
    let consensus_handle = tokio::spawn(async move {
        run_consensus(
            context,
            BlockNumber(1),
            *VALIDATOR_ID_1,
            Duration::ZERO,
            TIMEOUTS.clone(),
            &mut network_receiver,
            &mut sync_receiver,
        )
        .await
    });

    // Send sync for height 1.
    sync_sender.send(BlockNumber(1)).await.unwrap();
    // Make sure the sync is processed before the upcoming messages.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Decision for height 2.
    decision_rx.await.unwrap();

    // Drop the sender to close consensus and gracefully shut down.
    drop(sync_sender);
    assert!(matches!(consensus_handle.await.unwrap(), Err(ConsensusError::SyncError(_))));
}

// Check for cancellation safety when ignoring old heights. If the current height check was done
// within the select branch this test would hang.
#[tokio::test]
async fn run_consensus_sync_cancellation_safety() {
    let mut context = MockTestContext::new();
    let (proposal_handled_tx, proposal_handled_rx) = oneshot::channel();
    let (decision_tx, decision_rx) = oneshot::channel();

    context.expect_validate_proposal().return_once(move |_, _| {
        let (block_sender, block_receiver) = oneshot::channel();
        block_sender.send(TestBlock { content: vec![], id: BlockHash(Felt::ONE) }).unwrap();
        block_receiver
    });
    context.expect_validators().returning(move |_| vec![*PROPOSER_ID, *VALIDATOR_ID_1]);
    context.expect_proposer().returning(move |_, _| *PROPOSER_ID);
    context
        .expect_broadcast()
        .with(eq(prevote(Some(Felt::ONE), 1, 0, *VALIDATOR_ID_1)))
        .return_once(move |_| {
            proposal_handled_tx.send(()).unwrap();
            Ok(())
        });
    context.expect_broadcast().returning(move |_| Ok(()));
    context.expect_decision_reached().return_once(|block, votes| {
        assert_eq!(block.id(), BlockHash(Felt::ONE));
        assert_eq!(votes[0].height, 1);
        decision_tx.send(()).unwrap();
        Ok(())
    });

    let (mut network_sender, mut network_receiver) = mpsc::unbounded();
    let (mut sync_sender, mut sync_receiver) = mpsc::unbounded();

    let consensus_handle = tokio::spawn(async move {
        run_consensus(
            context,
            BlockNumber(1),
            *VALIDATOR_ID_1,
            Duration::ZERO,
            TIMEOUTS.clone(),
            &mut network_receiver,
            &mut sync_receiver,
        )
        .await
    });

    // Send a proposal for height 1.
    send(&mut network_sender, proposal(Felt::ONE, 1, 0, *PROPOSER_ID)).await;
    proposal_handled_rx.await.unwrap();

    // Send an old sync. This should not cancel the current height.
    sync_sender.send(BlockNumber(0)).await.unwrap();
    // Make sure the sync is processed before the upcoming messages.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Finished messages for 1
    send(&mut network_sender, prevote(Some(Felt::ONE), 1, 0, *PROPOSER_ID)).await;
    send(&mut network_sender, precommit(Some(Felt::ONE), 1, 0, *PROPOSER_ID)).await;
    decision_rx.await.unwrap();

    // Drop the sender to close consensus and gracefully shut down.
    drop(sync_sender);
    assert!(matches!(consensus_handle.await.unwrap(), Err(ConsensusError::SyncError(_))));
}

#[tokio::test]
async fn test_timeouts() {
    let (mut sender, mut receiver) = mpsc::unbounded();
    send(&mut sender, proposal(Felt::ONE, 1, 0, *PROPOSER_ID)).await;
    send(&mut sender, prevote(None, 1, 0, *VALIDATOR_ID_2)).await;
    send(&mut sender, prevote(None, 1, 0, *VALIDATOR_ID_3)).await;
    send(&mut sender, precommit(None, 1, 0, *VALIDATOR_ID_2)).await;
    send(&mut sender, precommit(None, 1, 0, *VALIDATOR_ID_3)).await;

    let mut context = MockTestContext::new();
    context.expect_validate_proposal().returning(move |_, _| {
        let (block_sender, block_receiver) = oneshot::channel();
        block_sender.send(TestBlock { content: vec![], id: BlockHash(Felt::ONE) }).unwrap();
        block_receiver
    });
    context
        .expect_validators()
        .returning(move |_| vec![*PROPOSER_ID, *VALIDATOR_ID_1, *VALIDATOR_ID_2, *VALIDATOR_ID_3]);
    context.expect_proposer().returning(move |_, _| *PROPOSER_ID);

    let (timeout_send, timeout_receive) = oneshot::channel();
    // Node handled Timeout events and responded with NIL vote.
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &ConsensusMessage| msg == &prevote(None, 1, 1, *VALIDATOR_ID_1))
        .return_once(move |_| {
            timeout_send.send(()).unwrap();
            Ok(())
        });
    context.expect_broadcast().returning(move |_| Ok(()));

    let mut manager = MultiHeightManager::new(*VALIDATOR_ID_1, TIMEOUTS.clone());
    let manager_handle = tokio::spawn(async move {
        let decision =
            manager.run_height(&mut context, BlockNumber(1), &mut receiver).await.unwrap();
        assert_eq!(decision.block.id(), BlockHash(Felt::ONE));
    });

    // Wait for the timeout to be triggered.
    timeout_receive.await.unwrap();
    // Show that after the timeout is triggered we can still precommit in favor of the block and
    // reach a decision.
    send(&mut sender, proposal(Felt::ONE, 1, 1, *PROPOSER_ID)).await;
    send(&mut sender, prevote(Some(Felt::ONE), 1, 1, *PROPOSER_ID)).await;
    send(&mut sender, prevote(Some(Felt::ONE), 1, 1, *VALIDATOR_ID_2)).await;
    send(&mut sender, prevote(Some(Felt::ONE), 1, 1, *VALIDATOR_ID_3)).await;
    send(&mut sender, precommit(Some(Felt::ONE), 1, 1, *VALIDATOR_ID_2)).await;
    send(&mut sender, precommit(Some(Felt::ONE), 1, 1, *VALIDATOR_ID_3)).await;

    manager_handle.await.unwrap();
}
