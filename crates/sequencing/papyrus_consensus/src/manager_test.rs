use std::vec;

use async_trait::async_trait;
use futures::channel::{mpsc, oneshot};
use futures::SinkExt;
use lazy_static::lazy_static;
use mockall::mock;
use papyrus_network::network_manager::ReportSender;
use papyrus_protobuf::consensus::{ConsensusMessage, Proposal, Vote, VoteType};
use papyrus_protobuf::converters::ProtobufConversionError;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::transaction::Transaction;
use starknet_types_core::felt::Felt;

use super::Manager;
use crate::types::{ConsensusBlock, ConsensusContext, ConsensusError, ProposalInit, ValidatorId};

lazy_static! {
    static ref VALIDATOR_ID: ValidatorId = 1_u32.into();
    static ref PROPOSER_ID: ValidatorId = 0_u32.into();
}

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

        fn proposer(&self, validators: &[ValidatorId], height: BlockNumber) -> ValidatorId;

        async fn broadcast(&mut self, message: ConsensusMessage) -> Result<(), ConsensusError>;

        async fn propose(
            &self,
            init: ProposalInit,
            content_receiver: mpsc::Receiver<Transaction>,
            fin_receiver: oneshot::Receiver<BlockHash>,
        ) -> Result<(), ConsensusError>;
    }
}

type Sender =
    mpsc::UnboundedSender<(Result<ConsensusMessage, ProtobufConversionError>, ReportSender)>;

async fn propose(sender: &mut Sender, block_hash: BlockHash, height: u64) {
    let msg = ConsensusMessage::Proposal(Proposal {
        height,
        block_hash,
        round: 0,
        proposer: *PROPOSER_ID,
        transactions: vec![],
    });

    sender.send((Ok(msg), oneshot::channel().0)).await.unwrap();
}

async fn prevote(
    sender: &mut Sender,
    block_hash: Option<BlockHash>,
    height: u64,
    voter: ValidatorId,
) {
    let msg = ConsensusMessage::Vote(Vote {
        vote_type: VoteType::Prevote,
        height,
        round: 0,
        block_hash,
        voter,
    });
    sender.send((Ok(msg), oneshot::channel().0)).await.unwrap();
}

async fn precommit(
    sender: &mut Sender,
    block_hash: Option<BlockHash>,
    height: u64,
    voter: ValidatorId,
) {
    let msg = ConsensusMessage::Vote(Vote {
        vote_type: VoteType::Precommit,
        height,
        round: 0,
        block_hash,
        voter,
    });
    sender.send((Ok(msg), oneshot::channel().0)).await.unwrap();
}

#[tokio::test]
async fn run_multiple_heights() {
    let mut context = MockTestContext::new();

    let (mut sender, mut receiver) = mpsc::unbounded();
    // Send messages for height 2 followed by those for height 1.
    propose(&mut sender, BlockHash(Felt::TWO), 2).await;
    prevote(&mut sender, Some(BlockHash(Felt::TWO)), 2, *PROPOSER_ID).await;
    precommit(&mut sender, Some(BlockHash(Felt::TWO)), 2, *PROPOSER_ID).await;
    propose(&mut sender, BlockHash(Felt::ONE), 1).await;
    prevote(&mut sender, Some(BlockHash(Felt::ONE)), 1, *PROPOSER_ID).await;
    precommit(&mut sender, Some(BlockHash(Felt::ONE)), 1, *PROPOSER_ID).await;

    let mut manager = Manager::new();

    // Run the manager for height 1.
    context
        .expect_validate_proposal()
        .return_once(move |_, _| {
            let (block_sender, block_receiver) = oneshot::channel();
            block_sender.send(TestBlock { content: vec![], id: BlockHash(Felt::ONE) }).unwrap();
            block_receiver
        })
        .times(1);
    context.expect_validators().returning(move |_| vec![*PROPOSER_ID, *VALIDATOR_ID]);
    context.expect_proposer().returning(move |_, _| *PROPOSER_ID);
    context.expect_broadcast().returning(move |_| Ok(()));
    let decision = manager
        .run_height(&mut context, BlockNumber(1), *VALIDATOR_ID, &mut receiver)
        .await
        .unwrap();
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
    let decision = manager
        .run_height(&mut context, BlockNumber(2), *VALIDATOR_ID, &mut receiver)
        .await
        .unwrap();
    assert_eq!(decision.block.id(), BlockHash(Felt::TWO));
}
