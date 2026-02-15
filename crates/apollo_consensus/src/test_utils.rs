use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use apollo_protobuf::consensus::{
    BuildParam,
    ProposalCommitment,
    ProposalInit,
    ProposalPart,
    Round,
    SignedProposalPart,
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
pub enum TestSignedProposalPart {
    Init(ProposalInit),
    Invalid,
}

impl From<ProposalInit> for TestSignedProposalPart {
    fn from(init: ProposalInit) -> Self {
        TestSignedProposalPart::Init(init)
    }
}

impl TryFrom<TestSignedProposalPart> for ProposalInit {
    type Error = ProtobufConversionError;
    fn try_from(part: TestSignedProposalPart) -> Result<Self, Self::Error> {
        if let TestSignedProposalPart::Init(init) = part {
            return Ok(init);
        }
        Err(ProtobufConversionError::SerdeJsonError("Invalid proposal part".to_string()))
    }
}

impl From<TestSignedProposalPart> for Vec<u8> {
    fn from(part: TestSignedProposalPart) -> Vec<u8> {
        if let TestSignedProposalPart::Init(init) = part {
            return init.into();
        }
        vec![]
    }
}

impl TryFrom<Vec<u8>> for TestSignedProposalPart {
    type Error = ProtobufConversionError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        Ok(TestSignedProposalPart::Init(value.try_into()?))
    }
}

impl TryFrom<TestSignedProposalPart> for SignedProposalPart {
    type Error = ProtobufConversionError;

    fn try_from(part: TestSignedProposalPart) -> Result<Self, Self::Error> {
        match part {
            TestSignedProposalPart::Init(init) => Ok(SignedProposalPart::init(init)),
            TestSignedProposalPart::Invalid => {
                Err(ProtobufConversionError::SerdeJsonError("Invalid proposal part".to_string()))
            }
        }
    }
}

impl From<SignedProposalPart> for TestSignedProposalPart {
    fn from(signed: SignedProposalPart) -> Self {
        match signed.part {
            ProposalPart::Init(init) => TestSignedProposalPart::Init(init),
            _ => TestSignedProposalPart::Invalid,
        }
    }
}

/// Alias for tests that use the legacy name.
pub type TestProposalPart = TestSignedProposalPart;

// TODO(matan): When QSelf is supported, switch to automocking `ConsensusContext`.
mock! {
    pub TestContext {}

    #[async_trait]
    impl ConsensusContext for TestContext {
        type SignedProposalPart = TestSignedProposalPart;

        async fn build_proposal(
            &mut self,
            build_param: BuildParam,
            timeout: Duration,
        ) -> Result<oneshot::Receiver<ProposalCommitment>, ConsensusError>;

        async fn validate_proposal(
            &mut self,
            init: ProposalInit,
            timeout: Duration,
            content: mpsc::Receiver<TestSignedProposalPart>
        ) -> oneshot::Receiver<ProposalCommitment>;

        async fn repropose(
            &mut self,
            id: ProposalCommitment,
            build_param: BuildParam,
        );

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

pub fn build_param(height: BlockNumber, round: Round, proposer: ValidatorId) -> BuildParam {
    BuildParam { height, round, proposer, ..Default::default() }
}

pub fn proposal_init(height: BlockNumber, round: Round, proposer: ValidatorId) -> ProposalInit {
    ProposalInit { height, round, proposer, ..Default::default() }
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
    mock.expect_get_proposer().returning(move |_, round| (*get_virtual)(round));
    mock.expect_get_actual_proposer().returning(move |_, round| (*get_actual)(round));
    Arc::new(mock)
}

/// Committee where virtual proposer equals actual proposer. Takes a single function that returns
/// the proposer address for a round (no Result). Use when both proposers are the same.
pub fn mock_committee_virtual_equal_to_actual(
    validators: Vec<ValidatorId>,
    get_actual_proposer_fn: Box<dyn Fn(Round) -> ContractAddress + Send + Sync>,
) -> Arc<dyn CommitteeTrait> {
    let stakers = validators
        .into_iter()
        .map(|address| Staker { address, weight: StakingWeight(1), public_key: Felt::ZERO })
        .collect();

    let get_actual = Arc::new(get_actual_proposer_fn);

    let mut mock = MockCommitteeTrait::new();
    mock.expect_members().return_const(stakers);
    let get_for_proposer = Arc::clone(&get_actual);
    mock.expect_get_proposer().returning(move |_, round| Ok((*get_for_proposer)(round)));
    mock.expect_get_actual_proposer().returning(move |_, round| (*get_actual)(round));
    Arc::new(mock)
}
