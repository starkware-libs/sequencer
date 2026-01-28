use std::sync::Arc;

use apollo_config_manager_types::communication::SharedConfigManagerClient;
use apollo_staking_config::config::{find_config_for_epoch, ConfiguredStaker, StakersConfig};
use apollo_state_sync_types::communication::SharedStateSyncClient;
use async_trait::async_trait;
use starknet_api::block::BlockNumber;
use tokio::sync::Mutex;
use tracing::warn;

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

    // A dynamically updateable timeline of staker configurations.
    // Each config entry applies from its defined start_epoch until the start epoch of another
    // entry overrides it.
    stakers_config: Arc<Mutex<Vec<StakersConfig>>>,
    config_manager_client: Option<SharedConfigManagerClient>,
}

impl MockStakingContract {
    /// Fixed epoch length used by the mock implementation.
    pub const EPOCH_LENGTH: u64 = 30;

    pub fn new(
        state_sync_client: SharedStateSyncClient,
        stakers_config: Vec<StakersConfig>,
        config_manager_client: Option<SharedConfigManagerClient>,
    ) -> Self {
        Self {
            state_sync_client,
            stakers_config: Arc::new(Mutex::new(stakers_config)),
            config_manager_client,
        }
    }

    /// Updates the stakers config from the config manager if available.
    async fn update_stakers_config(&self, stakers_config: &mut Vec<StakersConfig>) {
        let Some(client) = &self.config_manager_client else {
            return;
        };
        let dynamic_config = client.get_staking_manager_dynamic_config().await;
        match dynamic_config {
            Ok(dynamic_config) => {
                *stakers_config = dynamic_config.stakers_config;
            }
            Err(e) => {
                warn!("Failed to get stakers config from config manager: {e}");
            }
        }
    }
}

#[async_trait]
impl StakingContract for MockStakingContract {
    async fn get_stakers(&self, epoch: u64) -> StakingContractResult<Vec<Staker>> {
        let mut stakers_config = self.stakers_config.lock().await;
        self.update_stakers_config(&mut stakers_config).await;

        let config_entry = find_config_for_epoch(&stakers_config, epoch);

        if config_entry.is_none() {
            warn!("No stakers config available for epoch {epoch}");
            return Ok(vec![]);
        }
        Ok(config_entry.unwrap().stakers.iter().map(Staker::from).collect())
    }

    async fn get_current_epoch(&self) -> StakingContractResult<Epoch> {
        let latest_block_number =
            self.state_sync_client.get_latest_block_number().await?.unwrap_or(BlockNumber(0));

        let epoch_id = latest_block_number.0 / Self::EPOCH_LENGTH;
        let start_block = BlockNumber(epoch_id * Self::EPOCH_LENGTH);

        Ok(Epoch { epoch_id, start_block, epoch_length: Self::EPOCH_LENGTH })
    }
}
