use std::time::Duration;

use async_trait::async_trait;
use futures::channel::{mpsc, oneshot};
use mockall::mock;
use papyrus_protobuf::consensus::{ConsensusMessage, Proposal, Vote, VoteType};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_types_core::felt::Felt;

use crate::types::{
    ConsensusContext,
    ConsensusError,
    ProposalContentId,
    ProposalInit,
    Round,
    ValidatorId,
};

/// Define a consensus block which can be used to enable auto mocking Context.
#[derive(Debug, PartialEq, Clone)]
pub struct TestBlock {
    pub content: Vec<u32>,
    pub id: BlockHash,
}

// TODO(matan): When QSelf is supported, switch to automocking `ConsensusContext`.
mock! {
    pub TestContext {}

    #[async_trait]
    impl ConsensusContext for TestContext {
        type ProposalChunk = u32;

        async fn build_proposal(&mut self, height: BlockNumber, timeout: Duration) -> (
            mpsc::Receiver<u32>,
            oneshot::Receiver<ProposalContentId>
        );

        async fn validate_proposal(
            &mut self,
            height: BlockNumber,
            timeout: Duration,
            content: mpsc::Receiver<u32>
        ) -> oneshot::Receiver<ProposalContentId>;

        async fn repropose(
            &self,
            id: ProposalContentId,
            init: ProposalInit,
        );

        async fn validators(&self, height: BlockNumber) -> Vec<ValidatorId>;

        fn proposer(&self, height: BlockNumber, round: Round) -> ValidatorId;

        async fn broadcast(&mut self, message: ConsensusMessage) -> Result<(), ConsensusError>;

        async fn propose(
            &self,
            init: ProposalInit,
            content_receiver: mpsc::Receiver<u32>,
            fin_receiver: oneshot::Receiver<BlockHash>,
        ) -> Result<(), ConsensusError>;

        async fn decision_reached(
            &mut self,
            block: ProposalContentId,
            precommits: Vec<Vote>,
        ) -> Result<(), ConsensusError>;
    }
}

pub fn prevote(
    block_felt: Option<Felt>,
    height: u64,
    round: u32,
    voter: ValidatorId,
) -> ConsensusMessage {
    let block_hash = block_felt.map(BlockHash);
    ConsensusMessage::Vote(Vote { vote_type: VoteType::Prevote, height, round, block_hash, voter })
}

pub fn precommit(
    block_felt: Option<Felt>,
    height: u64,
    round: u32,
    voter: ValidatorId,
) -> ConsensusMessage {
    let block_hash = block_felt.map(BlockHash);
    ConsensusMessage::Vote(Vote {
        vote_type: VoteType::Precommit,
        height,
        round,
        block_hash,
        voter,
    })
}

pub fn proposal(
    block_felt: Felt,
    height: u64,
    round: u32,
    proposer: ValidatorId,
) -> ConsensusMessage {
    let block_hash = BlockHash(block_felt);
    ConsensusMessage::Proposal(Proposal {
        height,
        block_hash,
        round,
        proposer,
        transactions: Vec::new(),
        valid_round: None,
    })
}
