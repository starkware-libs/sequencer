use std::time::Duration;

use apollo_protobuf::consensus::{ProposalInit, Vote, VoteType};
use apollo_protobuf::converters::ProtobufConversionError;
use async_trait::async_trait;
use futures::channel::{mpsc, oneshot};
use mockall::mock;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_types_core::felt::Felt;

use crate::types::{ConsensusContext, ConsensusError, ProposalCommitment, Round, ValidatorId};

/// Define a consensus block which can be used to enable auto mocking Context.
#[derive(Debug, PartialEq, Clone)]
pub struct TestBlock {
    pub content: Vec<u32>,
    pub id: BlockHash,
}

#[derive(Debug, PartialEq, Clone)]
pub enum TestProposalPart {
    Init(ProposalInit),
}

impl From<ProposalInit> for TestProposalPart {
    fn from(init: ProposalInit) -> Self {
        TestProposalPart::Init(init)
    }
}

impl TryFrom<TestProposalPart> for ProposalInit {
    type Error = ProtobufConversionError;
    fn try_from(part: TestProposalPart) -> Result<Self, Self::Error> {
        let TestProposalPart::Init(init) = part;
        Ok(init)
    }
}

impl From<TestProposalPart> for Vec<u8> {
    fn from(part: TestProposalPart) -> Vec<u8> {
        let TestProposalPart::Init(init) = part;
        init.into()
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
        ) -> oneshot::Receiver<ProposalCommitment>;

        async fn validate_proposal(
            &mut self,
            init: ProposalInit,
            timeout: Duration,
            content: mpsc::Receiver<TestProposalPart>
        ) -> oneshot::Receiver<ProposalCommitment>;

        async fn repropose(
            &mut self,
            id: ProposalCommitment,
            init: ProposalInit,
        );

        async fn validators(&self, height: BlockNumber) -> Vec<ValidatorId>;

        fn proposer(&self, height: BlockNumber, round: Round) -> ValidatorId;

        async fn broadcast(&mut self, message: Vote) -> Result<(), ConsensusError>;

        async fn decision_reached(
            &mut self,
            block: ProposalCommitment,
            precommits: Vec<Vote>,
        ) -> Result<(), ConsensusError>;

        async fn try_sync(&mut self, height: BlockNumber) -> bool;

        async fn set_height_and_round(&mut self, height: BlockNumber, round: Round);
    }
}

pub fn prevote(block_felt: Option<Felt>, height: u64, round: u32, voter: ValidatorId) -> Vote {
    let proposal_commitment = block_felt.map(BlockHash);
    Vote { vote_type: VoteType::Prevote, height, round, proposal_commitment, voter }
}

pub fn precommit(block_felt: Option<Felt>, height: u64, round: u32, voter: ValidatorId) -> Vote {
    let proposal_commitment = block_felt.map(BlockHash);
    Vote { vote_type: VoteType::Precommit, height, round, proposal_commitment, voter }
}
pub fn proposal_init(height: u64, round: u32, proposer: ValidatorId) -> ProposalInit {
    ProposalInit { height: BlockNumber(height), round, proposer, ..Default::default() }
}
