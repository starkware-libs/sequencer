use std::time::Duration;

use async_trait::async_trait;
use futures::channel::{mpsc, oneshot};
use mockall::mock;
use papyrus_protobuf::consensus::{ConsensusMessage, ProposalFin, ProposalInit, Vote, VoteType};
use papyrus_protobuf::converters::ProtobufConversionError;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_types_core::felt::Felt;

use crate::types::{ConsensusContext, ConsensusError, ProposalContentId, Round, ValidatorId};

/// Define a consensus block which can be used to enable auto mocking Context.
#[derive(Debug, PartialEq, Clone)]
pub struct TestBlock {
    pub content: Vec<u32>,
    pub id: BlockHash,
}

#[derive(Debug, PartialEq, Clone)]
pub struct MockProposalPart(pub u64);

impl From<ProposalInit> for MockProposalPart {
    fn from(init: ProposalInit) -> Self {
        MockProposalPart(init.height.0)
    }
}

impl TryFrom<MockProposalPart> for ProposalInit {
    type Error = ProtobufConversionError;
    fn try_from(part: MockProposalPart) -> Result<Self, Self::Error> {
        Ok(ProposalInit {
            height: BlockNumber(part.0 as u64),
            round: 0,
            proposer: ValidatorId::default(),
            valid_round: None,
        })
    }
}

impl Into<Vec<u8>> for MockProposalPart {
    fn into(self) -> Vec<u8> {
        vec![self.0 as u8]
    }
}

impl TryFrom<Vec<u8>> for MockProposalPart {
    type Error = ProtobufConversionError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        Ok(MockProposalPart(value[0].into()))
    }
}

// TODO(matan): When QSelf is supported, switch to automocking `ConsensusContext`.
mock! {
    pub TestContext {}

    #[async_trait]
    impl ConsensusContext for TestContext {
        type ProposalChunk = u32;
        type ProposalPart = MockProposalPart;

        async fn build_proposal(
            &mut self,
            init: ProposalInit,
            timeout: Duration,
        ) -> oneshot::Receiver<ProposalContentId>;

        async fn validate_proposal(
            &mut self,
            height: BlockNumber,
            round: Round,
            timeout: Duration,
            content: mpsc::Receiver<MockProposalPart>
        ) -> oneshot::Receiver<(ProposalContentId, ProposalContentId)>;

        async fn repropose(
            &mut self,
            id: ProposalContentId,
            init: ProposalInit,
        );

        async fn validators(&self, height: BlockNumber) -> Vec<ValidatorId>;

        fn proposer(&self, height: BlockNumber, round: Round) -> ValidatorId;

        async fn broadcast(&mut self, message: ConsensusMessage) -> Result<(), ConsensusError>;

        async fn decision_reached(
            &mut self,
            block: ProposalContentId,
            precommits: Vec<Vote>,
        ) -> Result<(), ConsensusError>;

        async fn set_height_and_round(&mut self, height: BlockNumber, round: Round);
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

pub fn proposal_init(height: u64, round: u32, proposer: ValidatorId) -> ProposalInit {
    ProposalInit { height: BlockNumber(height), round, proposer, valid_round: None }
}

pub fn proposal_fin(block_felt: Felt) -> ProposalFin {
    ProposalFin { proposal_content_id: BlockHash(block_felt) }
}
