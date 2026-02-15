use std::sync::Arc;

use apollo_config_manager_types::communication::SharedConfigManagerClient;
use apollo_staking_config::config::{get_config_for_epoch, CommitteeConfig, ConfiguredStaker};
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

    // A dynamically updateable committee configuration with default and optional override.
    // The default applies to all epochs, while the override takes precedence
    // for epochs >= override.start_epoch.
    default_committee: Arc<Mutex<CommitteeConfig>>,
    override_committee: Arc<Mutex<Option<CommitteeConfig>>>,
    config_manager_client: Option<SharedConfigManagerClient>,
}

impl MockStakingContract {
    /// Fixed epoch length used by the mock implementation.
    pub const EPOCH_LENGTH: u64 = 30;

    pub fn new(
        state_sync_client: SharedStateSyncClient,
        default_committee: CommitteeConfig,
        override_committee: Option<CommitteeConfig>,
        config_manager_client: Option<SharedConfigManagerClient>,
    ) -> Self {
        Self {
            state_sync_client,
            default_committee: Arc::new(Mutex::new(default_committee)),
            override_committee: Arc::new(Mutex::new(override_committee)),
            config_manager_client,
        }
    }

    /// Updates the committee config from the config manager if available.
    async fn update_committee_config(
        &self,
        default_committee: &mut CommitteeConfig,
        override_committee: &mut Option<CommitteeConfig>,
    ) {
        let Some(client) = &self.config_manager_client else {
            return;
        };
        let dynamic_config = client.get_staking_manager_dynamic_config().await;
        match dynamic_config {
            Ok(dynamic_config) => {
                *default_committee = dynamic_config.default_committee;
                *override_committee = dynamic_config.override_committee;
            }
            Err(e) => {
                warn!("Failed to get committee config from config manager: {e}");
            }
        }
    }
}

#[async_trait]
impl StakingContract for MockStakingContract {
    async fn get_stakers(&self, epoch: u64) -> StakingContractResult<Vec<Staker>> {
        let mut default_committee = self.default_committee.lock().await;
        let mut override_committee = self.override_committee.lock().await;
        self.update_committee_config(&mut default_committee, &mut override_committee).await;

        let config = get_config_for_epoch(&default_committee, &override_committee, epoch);

        Ok(config.stakers.iter().map(Staker::from).collect())
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
