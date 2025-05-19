use std::sync::Arc;

use blockifier::context::BlockContext;
use blockifier::state::state_api::StateReader;
use starknet_api::core::ContractAddress;
use starknet_api::staking::StakingWeight;
use starknet_types_core::felt::Felt;
use thiserror::Error;

pub struct CommitteeManagerConfig {
    pub staking_contract_address: ContractAddress,
}

// Responsible for fetching and storing the committee at a given epoch.
// The committee is a subset of nodes (proposer and validators) that are selected to participate in
// the consensus at a given epoch, responsible for proposing blocks and voting on them.
pub struct CommitteeManager {
    #[allow(dead_code)]
    config: CommitteeManagerConfig,
}

#[derive(Debug, Error)]
pub enum CommitteeManagerError {}

pub type CommitteeManagerResult<T> = Result<T, CommitteeManagerError>;

impl CommitteeManager {
    pub fn new(config: CommitteeManagerConfig) -> Self {
        Self { config }
    }

    // Returns a list of the committee members at the given epoch.
    // The state's most recent block should be provided in the block_context.
    pub fn get_committee_at_epoch(
        &self,
        _epoch: u64,
        _state_reader: impl StateReader,
        _block_context: Arc<BlockContext>,
    ) -> CommitteeManagerResult<Vec<Staker>> {
        unimplemented!()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Staker {
    // A contract address of the staker, to which rewards are sent.
    pub address: ContractAddress,
    // The staker's weight, which determines the staker's influence in the consensus (its voting
    // power).
    pub weight: StakingWeight,
    // The public key of the staker, used to verify the staker's identity.
    pub public_key: Felt,
}
