use async_trait::async_trait;
use papyrus_base_layer::PriceSample;
use papyrus_proc_macros::handle_all_response_variants;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockNumber, BlockTimestamp};
use starknet_sequencer_infra::component_client::ClientError;
use starknet_sequencer_infra::component_definitions::ComponentClient;
use thiserror::Error;
use tracing::instrument;

pub mod communication;
pub mod l1_gas_price_provider;
pub mod l1_gas_price_scraper;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum L1GasPriceRequest {
    GetGasPrice(BlockTimestamp),
    AddGasPrice(BlockNumber, PriceSample),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum L1GasPriceResponse {
    GetGasPrice(L1GasPriceProviderResult<(u128, u128)>),
    AddGasPrice(L1GasPriceProviderResult<()>),
}

/// Serves as the provider's shared interface. Requires `Send + Sync` to allow transferring and
/// sharing resources (inputs, futures) across threads.
#[async_trait]
pub trait L1GasPriceProviderClient: Send + Sync {
    async fn add_price_info(
        &self,
        height: BlockNumber,
        sample: PriceSample,
    ) -> L1GasPriceProviderClientResult<()>;

    async fn get_price_info(
        &self,
        timestamp: BlockTimestamp,
    ) -> L1GasPriceProviderClientResult<(u128, u128)>;
}

#[async_trait]
impl<ComponentClientType> L1GasPriceProviderClient for ComponentClientType
where
    ComponentClientType: Send + Sync + ComponentClient<L1GasPriceRequest, L1GasPriceResponse>,
{
    #[instrument(skip(self))]
    async fn add_price_info(
        &self,
        height: BlockNumber,
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
    ) -> L1GasPriceProviderClientResult<(u128, u128)> {
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

#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum L1GasPriceProviderError {
    #[error("Failed to add price info: {0}")]
    InvalidHeight(String),
    #[error("Failed to add price info: {0}")]
    MissingData(String),
    #[error("Failed to get price info: {0}")]
    GetPriceInfoError(String),
}

#[derive(Clone, Debug, Error)]
pub enum L1GasPriceClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    L1GasPriceProviderError(#[from] L1GasPriceProviderError),
}

pub type L1GasPriceProviderResult<T> = Result<T, L1GasPriceProviderError>;
pub type L1GasPriceProviderClientResult<T> = Result<T, L1GasPriceClientError>;
