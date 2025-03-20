pub mod errors;

use std::sync::Arc;

use async_trait::async_trait;
use errors::{EthToStrkOracleClientError, L1GasPriceClientError, L1GasPriceProviderError};
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use papyrus_base_layer::{L1BlockNumber, PriceSample};
use papyrus_proc_macros::handle_all_response_variants;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockTimestamp;
use starknet_sequencer_infra::component_client::ClientError;
use starknet_sequencer_infra::component_definitions::ComponentClient;
use tracing::instrument;

pub type SharedL1GasPriceClient = Arc<dyn L1GasPriceProviderClient>;
pub type L1GasPriceProviderResult<T> = Result<T, L1GasPriceProviderError>;
pub type L1GasPriceProviderClientResult<T> = Result<T, L1GasPriceClientError>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PriceInfo {
    pub base_fee_per_gas: u128,
    pub blob_fee: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum L1GasPriceRequest {
    GetGasPrice(BlockTimestamp),
    AddGasPrice(L1BlockNumber, PriceSample),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum L1GasPriceResponse {
    GetGasPrice(L1GasPriceProviderResult<PriceInfo>),
    AddGasPrice(L1GasPriceProviderResult<()>),
}

/// Serves as the provider's shared interface. Requires `Send + Sync` to allow transferring and
/// sharing resources (inputs, futures) across threads.
#[cfg_attr(any(feature = "testing", test), automock)]
#[async_trait]
pub trait L1GasPriceProviderClient: Send + Sync {
    async fn add_price_info(
        &self,
        height: L1BlockNumber,
        sample: PriceSample,
    ) -> L1GasPriceProviderClientResult<()>;

    async fn get_price_info(
        &self,
        timestamp: BlockTimestamp,
    ) -> L1GasPriceProviderClientResult<PriceInfo>;
}

#[cfg_attr(any(feature = "testing", test), automock)]
#[async_trait]
pub trait EthToStrkOracleClientTrait: Send + Sync {
    /// Fetches the eth to fri rate for a given timestamp.
    async fn eth_to_fri_rate(&self, timestamp: u64) -> Result<u128, EthToStrkOracleClientError>;
}

#[async_trait]
impl<ComponentClientType> L1GasPriceProviderClient for ComponentClientType
where
    ComponentClientType: Send + Sync + ComponentClient<L1GasPriceRequest, L1GasPriceResponse>,
{
    #[instrument(skip(self))]
    async fn add_price_info(
        &self,
        height: L1BlockNumber,
        sample: PriceSample,
    ) -> L1GasPriceProviderClientResult<()> {
        let request = L1GasPriceRequest::AddGasPrice(height, sample);
        handle_all_response_variants!(
            L1GasPriceResponse,
            AddGasPrice,
            L1GasPriceClientError,
            L1GasPriceProviderError,
            Direct
        )
    }
    #[instrument(skip(self))]
    async fn get_price_info(
        &self,
        timestamp: BlockTimestamp,
    ) -> L1GasPriceProviderClientResult<PriceInfo> {
        let request = L1GasPriceRequest::GetGasPrice(timestamp);
        handle_all_response_variants!(
            L1GasPriceResponse,
            GetGasPrice,
            L1GasPriceClientError,
            L1GasPriceProviderError,
            Direct
        )
    }
}
