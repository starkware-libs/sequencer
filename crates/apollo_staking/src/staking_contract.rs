use apollo_batcher_types::communication::BatcherClientError;
use apollo_cairo_utils::RetdataDeserializationError;
use apollo_staking_config::config::StakingManagerDynamicConfig;
use apollo_state_sync_types::communication::StateSyncClientError;
use async_trait::async_trait;
use blockifier::execution::errors::EntryPointExecutionError;
#[cfg(test)]
use mockall::automock;
use thiserror::Error;

use crate::committee_provider::Staker;
use crate::staking_manager::Epoch;

#[derive(Debug, Error)]
pub enum StakingContractError {
    #[error(transparent)]
    BatcherClientError(#[from] BatcherClientError),
    #[error(transparent)]
    EntryPointExecutionError(#[from] EntryPointExecutionError),
    #[error(transparent)]
    RetdataDeserializationError(#[from] RetdataDeserializationError),
    #[error(transparent)]
    StateSyncClientError(#[from] StateSyncClientError),
}

pub type StakingContractResult<T> = Result<T, StakingContractError>;

/// An abstraction layer for interacting with the on-chain Staking Contract.
#[cfg_attr(test, automock)]
#[async_trait]
pub trait StakingContract: Send + Sync {
    /// Fetches the list of valid stakers for the specified epoch.
    ///
    /// This method filters out invalid stakers (e.g., those without a public key).
    async fn get_stakers(&self, epoch: u64) -> StakingContractResult<Vec<Staker>>;

    /// Fetches the list of valid stakers for the specified epoch with injected config.
    ///
    /// This method allows the caller to provide dynamic configuration.
    /// Default implementation ignores the config and delegates to get_stakers().
    async fn get_stakers_with_config(
        &self,
        epoch: u64,
        _config: &StakingManagerDynamicConfig,
    ) -> StakingContractResult<Vec<Staker>> {
        // Default: ignore config and call the original method.
        self.get_stakers(epoch).await
    }

    /// Fetches the current epoch.
    async fn get_current_epoch(&self) -> StakingContractResult<Epoch>;

    /// Fetches the previous epoch.
    /// Returns None if there is no previous epoch (i.e., we are in the first epoch).
    async fn get_previous_epoch(&self) -> StakingContractResult<Option<Epoch>>;
}
