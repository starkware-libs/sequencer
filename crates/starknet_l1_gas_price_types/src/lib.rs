pub mod errors;

use std::sync::Arc;

use async_trait::async_trait;
use errors::{L1GasPriceClientError, L1GasPriceProviderError};
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use papyrus_base_layer::PriceSample;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockNumber, BlockTimestamp};

pub type SharedL1GasPriceClient = Arc<dyn L1GasPriceProviderClient>;
pub type L1GasPriceProviderResult<T> = Result<T, L1GasPriceProviderError>;
pub type L1GasPriceProviderClientResult<T> = Result<T, L1GasPriceClientError>;

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
#[cfg_attr(any(feature = "testing", test), automock)]
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
