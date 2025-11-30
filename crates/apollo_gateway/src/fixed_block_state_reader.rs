use std::sync::Arc;

use apollo_state_sync_types::communication::{SharedStateSyncClient, StateSyncClient};
use async_trait::async_trait;
use blockifier::state::errors::StateError;
use blockifier::state::state_api::StateResult;
use starknet_api::block::{BlockInfo, BlockNumber, GasPriceVector, GasPrices};
use starknet_api::data_availability::L1DataAvailabilityMode;
use tokio::sync::OnceCell;

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait FixedBlockSyncStateReader: Send + Sync {
    async fn get_block_info(&self) -> StateResult<BlockInfo>;
}

pub struct FixedBlockSyncStateClient {
    state_sync_client: Arc<dyn StateSyncClient>,
    block_number: BlockNumber,
    block_info_cache: OnceCell<BlockInfo>,
}

impl FixedBlockSyncStateClient {
    pub fn new(state_sync_client: SharedStateSyncClient, block_number: BlockNumber) -> Self {
        Self { state_sync_client, block_number, block_info_cache: OnceCell::new() }
    }

    async fn get_block_info_from_sync_client(&self) -> StateResult<BlockInfo> {
        let block = self
            .state_sync_client
            .get_block(self.block_number)
            .await
            .map_err(|e| StateError::StateReadError(e.to_string()))?;

        let block_header = block.block_header_without_hash;
        let block_info = BlockInfo {
            block_number: block_header.block_number,
            block_timestamp: block_header.timestamp,
            sequencer_address: block_header.sequencer.0,
            gas_prices: GasPrices {
                eth_gas_prices: GasPriceVector {
                    l1_gas_price: block_header.l1_gas_price.price_in_wei.try_into()?,
                    l1_data_gas_price: block_header.l1_data_gas_price.price_in_wei.try_into()?,
                    l2_gas_price: block_header.l2_gas_price.price_in_wei.try_into()?,
                },
                strk_gas_prices: GasPriceVector {
                    l1_gas_price: block_header.l1_gas_price.price_in_fri.try_into()?,
                    l1_data_gas_price: block_header.l1_data_gas_price.price_in_fri.try_into()?,
                    l2_gas_price: block_header.l2_gas_price.price_in_fri.try_into()?,
                },
            },
            use_kzg_da: match block_header.l1_da_mode {
                L1DataAvailabilityMode::Blob => true,
                L1DataAvailabilityMode::Calldata => false,
            },
        };

        Ok(block_info)
    }
}

#[async_trait]
impl FixedBlockSyncStateReader for FixedBlockSyncStateClient {
    async fn get_block_info(&self) -> StateResult<BlockInfo> {
        self.block_info_cache
            .get_or_try_init(|| self.get_block_info_from_sync_client())
            .await
            .cloned()
    }
}
