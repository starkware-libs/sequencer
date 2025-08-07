pub mod errors;
use std::fmt::Debug;
use std::iter::Sum;
use std::sync::Arc;

use apollo_infra::component_client::ClientError;
use apollo_infra::component_definitions::{ComponentClient, PrioritizedRequest};
use apollo_infra::impl_debug_for_infra_requests_and_responses;
use apollo_proc_macros::handle_all_response_variants;
use async_trait::async_trait;
use errors::{EthToStrkOracleClientError, L1GasPriceClientError, L1GasPriceProviderError};
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use papyrus_base_layer::L1BlockNumber;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockTimestamp, GasPrice};
use strum_macros::AsRefStr;
use tracing::instrument;

pub const DEFAULT_ETH_TO_FRI_RATE: u128 = 10_u128.pow(21);

pub type SharedL1GasPriceClient = Arc<dyn L1GasPriceProviderClient>;
pub type L1GasPriceProviderResult<T> = Result<T, L1GasPriceProviderError>;
pub type L1GasPriceProviderClientResult<T> = Result<T, L1GasPriceClientError>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GasPriceData {
    pub block_number: L1BlockNumber,
    pub timestamp: BlockTimestamp,
    pub price_info: PriceInfo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PriceInfo {
    pub base_fee_per_gas: GasPrice,
    pub blob_fee: GasPrice,
}

impl PriceInfo {
    pub fn checked_div(&self, divisor: u128) -> Option<PriceInfo> {
        let base_fee_per_gas = self.base_fee_per_gas.checked_div(divisor)?;
        let blob_fee = self.blob_fee.checked_div(divisor)?;
        Some(PriceInfo { base_fee_per_gas, blob_fee })
    }
}

impl<'a> Sum<&'a Self> for PriceInfo {
    fn sum<I>(iter: I) -> Self
    where
        I: Iterator<Item = &'a Self>,
    {
        iter.fold(Self { base_fee_per_gas: GasPrice(0), blob_fee: GasPrice(0) }, |a, b| Self {
            base_fee_per_gas: a.base_fee_per_gas.saturating_add(b.base_fee_per_gas),
            blob_fee: a.blob_fee.saturating_add(b.blob_fee),
        })
    }
}

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum L1GasPriceRequest {
    Initialize,
    GetGasPrice(BlockTimestamp),
    AddGasPrice(GasPriceData),
    GetEthToFriRate(u64),
}
impl_debug_for_infra_requests_and_responses!(L1GasPriceRequest);
impl PrioritizedRequest for L1GasPriceRequest {}

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum L1GasPriceResponse {
    Initialize(L1GasPriceProviderResult<()>),
    GetGasPrice(L1GasPriceProviderResult<PriceInfo>),
    AddGasPrice(L1GasPriceProviderResult<()>),
    GetEthToFriRate(L1GasPriceProviderResult<u128>),
}
impl_debug_for_infra_requests_and_responses!(L1GasPriceResponse);

/// Serves as the provider's shared interface. Requires `Send + Sync` to allow transferring and
/// sharing resources (inputs, futures) across threads.
#[cfg_attr(any(feature = "testing", test), automock)]
#[async_trait]
pub trait L1GasPriceProviderClient: Send + Sync {
    async fn initialize(&self) -> L1GasPriceProviderClientResult<()>;

    async fn add_price_info(&self, new_data: GasPriceData) -> L1GasPriceProviderClientResult<()>;

    async fn get_price_info(
        &self,
        timestamp: BlockTimestamp,
    ) -> L1GasPriceProviderClientResult<PriceInfo>;

    async fn get_eth_to_fri_rate(&self, timestamp: u64) -> L1GasPriceProviderClientResult<u128>;
}

#[cfg_attr(any(feature = "testing", test), automock)]
#[async_trait]
pub trait EthToStrkOracleClientTrait: Send + Sync + Debug {
    /// Fetches the eth to fri rate for a given timestamp.
    async fn eth_to_fri_rate(&self, timestamp: u64) -> Result<u128, EthToStrkOracleClientError>;
}

#[async_trait]
impl<ComponentClientType> L1GasPriceProviderClient for ComponentClientType
where
    ComponentClientType: Send + Sync + ComponentClient<L1GasPriceRequest, L1GasPriceResponse>,
{
    #[instrument(skip(self))]
    async fn initialize(&self) -> L1GasPriceProviderClientResult<()> {
        let request = L1GasPriceRequest::Initialize;
        handle_all_response_variants!(
            L1GasPriceResponse,
            Initialize,
            L1GasPriceClientError,
            L1GasPriceProviderError,
            Direct
        )
    }
    #[instrument(skip(self))]
    async fn add_price_info(&self, new_data: GasPriceData) -> L1GasPriceProviderClientResult<()> {
        let request = L1GasPriceRequest::AddGasPrice(new_data);
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
    #[instrument(skip(self))]
    async fn get_eth_to_fri_rate(&self, timestamp: u64) -> L1GasPriceProviderClientResult<u128> {
        let request = L1GasPriceRequest::GetEthToFriRate(timestamp);
        handle_all_response_variants!(
            L1GasPriceResponse,
            GetEthToFriRate,
            L1GasPriceClientError,
            L1GasPriceProviderError,
            Direct
        )
    }
}
