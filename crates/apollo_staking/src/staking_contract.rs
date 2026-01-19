#[cfg(test)]
use mockall::automock;

use crate::committee_provider::{CommitteeProviderResult, Staker};
use crate::staking_manager::Epoch;

/// An abstraction layer for interacting with the on-chain Staking Contract.
#[cfg_attr(test, automock)]
pub trait StakingContract: Send + Sync {
    /// Fetches the list of valid stakers for the specified epoch.
    ///
    /// This method filters out invalid stakers (e.g., those without a public key).
    fn get_stakers(&self, epoch: u64) -> CommitteeProviderResult<Vec<Staker>>;

    /// Fetches the current epoch.
    fn get_current_epoch(&self) -> CommitteeProviderResult<Epoch>;
}
