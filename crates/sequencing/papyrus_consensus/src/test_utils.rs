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
pub enum TestProposalPart {
    Init(ProposalInit),
    Fin(ProposalFin),
}

impl From<ProposalInit> for TestProposalPart {
    fn from(init: ProposalInit) -> Self {
        TestProposalPart::Init(init)
    }
}

impl TryFrom<TestProposalPart> for ProposalInit {
    type Error = ProtobufConversionError;
    fn try_from(part: TestProposalPart) -> Result<Self, Self::Error> {
        match part {
            TestProposalPart::Init(init) => Ok(init),
            _ => Err(ProtobufConversionError::WrongEnumVariant {
                type_description: "TestProposalPart",
                expected: "Init",
                value_as_str: format!("{:?}", part),
            }),
        }
    }
}

impl From<TestProposalPart> for Vec<u8> {
    fn from(part: TestProposalPart) -> Vec<u8> {
        let init = match part {
            TestProposalPart::Init(init) => init,
            _ => panic!("Invalid TestProposalPart conversion"),
        };
        <Vec<u8>>::try_from(init).expect("Invalid TestProposalPart conversion")
    }
}

impl TryFrom<Vec<u8>> for TestProposalPart {
    type Error = ProtobufConversionError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        Ok(TestProposalPart::Init(value.try_into()?))
    }
}

// TODO(matan): When QSelf is supported, switch to automocking `ConsensusContext`.
mock! {
    pub TestContext {}

    #[async_trait]
    impl ConsensusContext for TestContext {
        type ProposalPart = TestProposalPart;

        async fn build_proposal(
            &mut self,
            init: ProposalInit,
            timeout: Duration,
        ) -> oneshot::Receiver<ProposalContentId>;

        async fn validate_proposal(
            &mut self,
            height: BlockNumber,
            round: Round,
            proposer: ValidatorId,
            timeout: Duration,
            content: mpsc::Receiver<TestProposalPart>
        ) -> oneshot::Receiver<(ProposalContentId, ProposalFin)>;

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
