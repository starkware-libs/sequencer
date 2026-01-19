#[cfg(test)]
use mockall::automock;
use thiserror::Error;

use crate::committee_provider::Staker;
use crate::staking_manager::Epoch;

#[derive(Debug, Error)]
pub enum StakingContractError {}

pub type StakingContractResult<T> = Result<T, StakingContractError>;

/// An abstraction layer for interacting with the on-chain Staking Contract.
#[cfg_attr(test, automock)]
pub trait StakingContract: Send + Sync {
    /// Fetches the list of valid stakers for the specified epoch.
    ///
    /// This method filters out invalid stakers (e.g., those without a public key).
    fn get_stakers(&self, epoch: u64) -> StakingContractResult<Vec<Staker>>;

    /// Fetches the current epoch.
    fn get_current_epoch(&self) -> StakingContractResult<Epoch>;
}
