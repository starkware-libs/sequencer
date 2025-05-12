pub mod errors;

use std::sync::Arc;

use apollo_infra::component_client::ClientError;
use apollo_infra::component_definitions::ComponentClient;
use apollo_infra::impl_debug_for_infra_requests_and_responses;
use apollo_proc_macros::handle_all_response_variants;
use async_trait::async_trait;
use errors::{EthToStrkOracleClientError, L1GasPriceClientError, L1GasPriceProviderError};
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use papyrus_base_layer::{L1BlockNumber, PriceSample};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockTimestamp, GasPrice};
use strum_macros::AsRefStr;
use tracing::instrument;

pub const DEFAULT_ETH_TO_FRI_RATE: u128 = 10_u128.pow(21);

pub type SharedL1GasPriceClient = Arc<dyn L1GasPriceProviderClient>;
pub type L1GasPriceProviderResult<T> = Result<T, L1GasPriceProviderError>;
pub type L1GasPriceProviderClientResult<T> = Result<T, L1GasPriceClientError>;

#[derive(Clone, Debug)]
pub struct GasPriceData {
    pub height: L1BlockNumber,
    pub sample: PriceSample,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PriceInfo {
    pub base_fee_per_gas: GasPrice,
    pub blob_fee: GasPrice,
}

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum L1GasPriceRequest {
    GetGasPrice(BlockTimestamp),
    AddGasPrice(L1BlockNumber, PriceSample),
}
impl_debug_for_infra_requests_and_responses!(L1GasPriceRequest);

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum L1GasPriceResponse {
    GetGasPrice(L1GasPriceProviderResult<PriceInfo>),
    AddGasPrice(L1GasPriceProviderResult<()>),
}
impl_debug_for_infra_requests_and_responses!(L1GasPriceResponse);

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
