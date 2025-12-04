use std::sync::Arc;

use apollo_gateway_types::deprecated_gateway_error::StarknetError;
use apollo_state_sync_types::communication::{SharedStateSyncClient, StateSyncClient};
use async_trait::async_trait;
use starknet_api::block::{BlockInfo, BlockNumber, GasPriceVector, GasPrices};
use starknet_api::data_availability::L1DataAvailabilityMode;
use tokio::sync::OnceCell;

pub type StarknetResult<T> = Result<T, StarknetError>;

/// A reader to a fixed block in the synced state of Starknet.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait GatewayFixedBlockStateReader: Send + Sync {
    async fn get_block_info(&self) -> StarknetResult<BlockInfo>;
}

pub struct GatewayFixedBlockSyncStateClient {
    state_sync_client: Arc<dyn StateSyncClient>,
    block_number: BlockNumber,
    block_info_cache: OnceCell<BlockInfo>,
}

impl GatewayFixedBlockSyncStateClient {
    pub fn new(state_sync_client: SharedStateSyncClient, block_number: BlockNumber) -> Self {
        Self { state_sync_client, block_number, block_info_cache: OnceCell::new() }
    }

    async fn get_block_info_from_sync_client(&self) -> StarknetResult<BlockInfo> {
        let block = self.state_sync_client.get_block(self.block_number).await.map_err(|e| {
            StarknetError::internal_with_logging("Failed to get latest block info", e)
        })?;

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
impl GatewayFixedBlockStateReader for GatewayFixedBlockSyncStateClient {
    async fn get_block_info(&self) -> StarknetResult<BlockInfo> {
        self.block_info_cache
            .get_or_try_init(|| self.get_block_info_from_sync_client())
            .await
            .cloned()
    }
}
