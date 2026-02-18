use apollo_staking_config::config::{
    get_config_for_epoch,
    ConfiguredStaker,
    StakingManagerDynamicConfig,
};
use apollo_state_sync_types::communication::SharedStateSyncClient;
use async_trait::async_trait;
use starknet_api::block::BlockNumber;

use crate::committee_provider::Staker;
use crate::staking_contract::{StakingContract, StakingContractResult};
use crate::staking_manager::Epoch;

#[cfg(test)]
#[path = "mock_staking_contract_test.rs"]
mod mock_staking_contract_test;

impl From<&ConfiguredStaker> for Staker {
    fn from(configured_staker: &ConfiguredStaker) -> Self {
        Staker {
            address: configured_staker.address,
            weight: configured_staker.weight,
            public_key: configured_staker.public_key,
        }
    }
}

/// Mock implementation of the staking contract backed by static in-memory configuration.
pub struct MockStakingContract {
    state_sync_client: SharedStateSyncClient,
    // Default configuration used when no other configuration is provided.
    default_config: StakingManagerDynamicConfig,
}

impl MockStakingContract {
    /// Fixed epoch length used by the mock implementation.
    pub const EPOCH_LENGTH: u64 = 30;

    pub fn new(
        state_sync_client: SharedStateSyncClient,
        default_config: StakingManagerDynamicConfig,
    ) -> Self {
        Self { state_sync_client, default_config }
    }
}

#[async_trait]
impl StakingContract for MockStakingContract {
    async fn get_stakers(&self, epoch: u64) -> StakingContractResult<Vec<Staker>> {
        self.get_stakers_with_config(epoch, &self.default_config).await
    }

    async fn get_stakers_with_config(
        &self,
        epoch: u64,
        config: &StakingManagerDynamicConfig,
    ) -> StakingContractResult<Vec<Staker>> {
        let active_config =
            get_config_for_epoch(&config.default_committee, &config.override_committee, epoch);
        Ok(active_config.stakers.iter().map(Staker::from).collect())
    }

    async fn get_current_epoch(&self) -> StakingContractResult<Epoch> {
        let latest_block_number =
            self.state_sync_client.get_latest_block_number().await?.unwrap_or(BlockNumber(0));

        let epoch_id = latest_block_number.0 / Self::EPOCH_LENGTH;
        let start_block = BlockNumber(epoch_id * Self::EPOCH_LENGTH);

        Ok(Epoch { epoch_id, start_block, epoch_length: Self::EPOCH_LENGTH })
    }

    async fn get_previous_epoch(&self) -> StakingContractResult<Option<Epoch>> {
        let current_epoch = self.get_current_epoch().await?;

        if current_epoch.epoch_id == 0 {
            return Ok(None);
        }

        let previous_epoch_id = current_epoch.epoch_id - 1;
        let start_block = BlockNumber(previous_epoch_id * Self::EPOCH_LENGTH);

        Ok(Some(Epoch {
            epoch_id: previous_epoch_id,
            start_block,
            epoch_length: Self::EPOCH_LENGTH,
        }))
    }
}
