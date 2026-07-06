use apollo_batcher_types::communication::SharedBatcherClient;
use apollo_staking_config::config::{
    get_config_for_epoch,
    ConfiguredStaker,
    StakingManagerDynamicConfig,
};
use apollo_state_sync_types::communication::SharedStateSyncClient;
use async_trait::async_trait;
use starknet_api::block::BlockNumber;
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
    batcher_client: SharedBatcherClient,
    state_sync_client: SharedStateSyncClient,
    // Default configuration used when no other configuration is provided.
    default_config: StakingManagerDynamicConfig,
}

impl MockStakingContract {
    /// Fixed epoch length used by the mock implementation.
    pub const EPOCH_LENGTH: u64 = 30;

    pub fn new(
        batcher_client: SharedBatcherClient,
        state_sync_client: SharedStateSyncClient,
        default_config: StakingManagerDynamicConfig,
    ) -> Self {
        Self { batcher_client, state_sync_client, default_config }
    }

    // Returns the latest committed block number, preferring the batcher and falling back to state
    // sync on batcher error. The batcher's height marker tracks the commit tip, whereas state sync
    // is a downstream reader that can lag it significantly; deriving the epoch from a lagging
    // source can push the consensus height beyond the resolvable committee window and stall block
    // production. `get_height` returns the next-to-build marker, so the latest committed block is
    // one below it.
    async fn latest_committed_block(&self) -> StakingContractResult<BlockNumber> {
        match self.batcher_client.get_height().await {
            Ok(response) => Ok(BlockNumber(response.height.0.saturating_sub(1))),
            Err(batcher_error) => {
                warn!("Batcher get_height failed, falling back to state sync: {batcher_error}");
                Ok(self
                    .state_sync_client
                    .get_latest_block_number()
                    .await?
                    .unwrap_or(BlockNumber(0)))
            }
        }
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
        let latest_block_number = self.latest_committed_block().await?;

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
