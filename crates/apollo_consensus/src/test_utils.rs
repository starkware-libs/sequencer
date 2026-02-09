use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use apollo_protobuf::consensus::{
    ConsensusBlockInfo,
    ProposalCommitment,
    ProposalInit,
    Round,
    Vote,
    VoteType,
};
use apollo_protobuf::converters::ProtobufConversionError;
use apollo_staking::committee_provider::{
    CommitteeError,
    CommitteeTrait,
    MockCommitteeTrait,
    Staker,
};
use apollo_storage::db::DbConfig;
use apollo_storage::StorageConfig;
use async_trait::async_trait;
use futures::channel::{mpsc, oneshot};
use mockall::mock;
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_api::crypto::utils::RawSignature;
use starknet_api::staking::StakingWeight;
use starknet_types_core::felt::Felt;

use crate::storage::{HeightVotedStorageError, HeightVotedStorageTrait};
use crate::types::{ConsensusContext, ConsensusError, ValidatorId};

/// Define a consensus block which can be used to enable auto mocking Context.
#[derive(Debug, PartialEq, Clone)]
pub struct TestBlock {
    pub content: Vec<u32>,
    pub id: ProposalCommitment,
}

#[derive(Debug, PartialEq, Clone)]
pub enum TestProposalPart {
    BlockInfo(ConsensusBlockInfo),
    Invalid,
}

impl From<ConsensusBlockInfo> for TestProposalPart {
    fn from(block_info: ConsensusBlockInfo) -> Self {
        TestProposalPart::BlockInfo(block_info)
    }
}

impl TryFrom<TestProposalPart> for ConsensusBlockInfo {
    type Error = ProtobufConversionError;
    fn try_from(part: TestProposalPart) -> Result<Self, Self::Error> {
        if let TestProposalPart::BlockInfo(block_info) = part {
            return Ok(block_info);
        }
        Err(ProtobufConversionError::SerdeJsonError("Invalid proposal part".to_string()))
    }
}

impl From<TestProposalPart> for Vec<u8> {
    fn from(part: TestProposalPart) -> Vec<u8> {
        if let TestProposalPart::BlockInfo(block_info) = part {
            return block_info.into();
        }
        vec![]
    }
}

impl TryFrom<Vec<u8>> for TestProposalPart {
    type Error = ProtobufConversionError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        Ok(TestProposalPart::BlockInfo(value.try_into()?))
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
        ) -> Result<oneshot::Receiver<ProposalCommitment>, ConsensusError>;

        async fn validate_proposal(
            &mut self,
            block_info: ConsensusBlockInfo,
            timeout: Duration,
            content: mpsc::Receiver<TestProposalPart>
        ) -> oneshot::Receiver<ProposalCommitment>;

        async fn repropose(
            &mut self,
            id: ProposalCommitment,
            init: ProposalInit,
        );

        async fn validators(&self, height: BlockNumber) -> Result<Vec<ValidatorId>, ConsensusError>;

        fn proposer(&self, height: BlockNumber, round: Round) -> Result<ValidatorId, ConsensusError>;

        fn virtual_proposer(&self, height: BlockNumber, round: Round) -> Result<ValidatorId, ConsensusError>;

        async fn broadcast(&mut self, message: Vote) -> Result<(), ConsensusError>;

        async fn decision_reached(
            &mut self,
            height: BlockNumber,
            commitment: ProposalCommitment,
        ) -> Result<(), ConsensusError>;

        async fn try_sync(&mut self, height: BlockNumber) -> bool;

        async fn set_height_and_round(&mut self, height: BlockNumber, round: Round) -> Result<(), ConsensusError>;
    }
}

pub fn prevote(
    block_felt: Option<Felt>,
    height: BlockNumber,
    round: Round,
    voter: ValidatorId,
) -> Vote {
    let proposal_commitment = block_felt.map(ProposalCommitment);
    Vote {
        vote_type: VoteType::Prevote,
        height,
        round,
        proposal_commitment,
        voter,
        signature: RawSignature::default(),
    }
}

pub fn precommit(
    block_felt: Option<Felt>,
    height: BlockNumber,
    round: Round,
    voter: ValidatorId,
) -> Vote {
    let proposal_commitment = block_felt.map(ProposalCommitment);
    Vote {
        vote_type: VoteType::Precommit,
        height,
        round,
        proposal_commitment,
        voter,
        signature: RawSignature::default(),
    }
}

pub fn proposal_init(height: BlockNumber, round: Round, proposer: ValidatorId) -> ProposalInit {
    ProposalInit { height, round, proposer, ..Default::default() }
}

pub fn block_info(height: BlockNumber, round: Round, proposer: ValidatorId) -> ConsensusBlockInfo {
    ConsensusBlockInfo { height, round, proposer, ..Default::default() }
}

#[derive(Debug)]
pub struct NoOpHeightVotedStorage;

impl HeightVotedStorageTrait for NoOpHeightVotedStorage {
    fn get_prev_voted_height(&self) -> Result<Option<BlockNumber>, HeightVotedStorageError> {
        Ok(None)
    }
    fn set_prev_voted_height(
        &mut self,
        _height: BlockNumber,
    ) -> Result<(), HeightVotedStorageError> {
        Ok(())
    }
    fn revert_height(&mut self, _height: BlockNumber) -> Result<(), HeightVotedStorageError> {
        Ok(())
    }
}

/// Returns a config for a new (i.e. empty) storage.
pub fn get_new_storage_config() -> StorageConfig {
    static DB_INDEX: AtomicUsize = AtomicUsize::new(0);
    let db_file_path = format!(
        "{}-{}",
        tempfile::tempdir().unwrap().path().to_str().unwrap(),
        DB_INDEX.fetch_add(1, Ordering::Relaxed)
    );
    StorageConfig {
        db_config: DbConfig { path_prefix: PathBuf::from(db_file_path), ..Default::default() },
        ..Default::default()
    }
}

pub fn test_committee(
    validators: Vec<ValidatorId>,
    get_actual_proposer_fn: Box<dyn Fn(Round) -> ContractAddress + Send + Sync>,
    get_virtual_proposer_fn: Box<
        dyn Fn(Round) -> Result<ContractAddress, CommitteeError> + Send + Sync,
    >,
) -> Arc<dyn CommitteeTrait> {
    let stakers = validators
        .into_iter()
        .map(|address| Staker { address, weight: StakingWeight(1), public_key: Felt::ZERO })
        .collect();

    let get_actual = Arc::new(get_actual_proposer_fn);
    let get_virtual = Arc::new(get_virtual_proposer_fn);

    let mut mock = MockCommitteeTrait::new();
    mock.expect_members().return_const(stakers);
    mock.expect_get_proposer()
        .returning(move |_, round| (*get_virtual)(round));
    mock.expect_get_actual_proposer()
        .returning(move |_, round| (*get_actual)(round));
    Arc::new(mock)
}
