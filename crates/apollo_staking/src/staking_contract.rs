use std::sync::Arc;

use blockifier::context::BlockContext;
#[cfg(test)]
use mockall::automock;

use crate::committee_provider::{CommitteeProviderResult, Staker};
use crate::staking_manager::Epoch;

/// An abstraction layer for interacting with the on-chain Staking Contract.
#[cfg_attr(test, automock)]
pub trait StakingContract: Send + Sync {
    /// Fetches the list of valid stakers for the specified epoch.
    ///
    /// The data is retrieved from the contract state as of the provided `block_context`.
    /// This method filters out invalid stakers (e.g., those without a public key).
    fn get_stakers(
        &self,
        epoch: u64,
        block_context: Arc<BlockContext>,
    ) -> CommitteeProviderResult<Vec<Staker>>;

    /// Resolves the current epoch based on the provided block context.
    fn get_current_epoch(&self, block_context: Arc<BlockContext>)
    -> CommitteeProviderResult<Epoch>;
}
