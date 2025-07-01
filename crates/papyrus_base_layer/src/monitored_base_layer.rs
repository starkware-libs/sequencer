use std::ops::{Deref, DerefMut, RangeInclusive};

use apollo_l1_endpoint_monitor_types::{
    L1EndpointMonitorClientError,
    L1EndpointMonitorError,
    SharedL1EndpointMonitorClient,
};
use async_trait::async_trait;
use starknet_api::block::BlockHashAndNumber;
use tokio::sync::Mutex;
use tracing::{error, info};
use url::Url;

use crate::ethereum_base_layer_contract::EthereumBaseLayerContract;
use crate::{BaseLayerContract, L1BlockHeader, L1BlockNumber, L1BlockReference, L1Event};

pub type MonitoredEthereumBaseLayer = MonitoredBaseLayer<EthereumBaseLayerContract>;

// Using interior mutability for modifiable fields in order to comply with the base layer's
// largely immutable API.
pub struct MonitoredBaseLayer<B: BaseLayerContract + Send + Sync> {
    pub monitor: SharedL1EndpointMonitorClient,
    current_node_url: Mutex<Url>,
    base_layer: Mutex<B>,
}

impl<B: BaseLayerContract + Send + Sync> MonitoredBaseLayer<B> {
    pub fn new(
        base_layer: B,
        l1_endpoint_monitor_client: SharedL1EndpointMonitorClient,
        initial_node_url: Url,
    ) -> Self {
        MonitoredBaseLayer {
            base_layer: Mutex::new(base_layer),
            monitor: l1_endpoint_monitor_client,
            current_node_url: Mutex::new(initial_node_url),
        }
    }

    /// Returns a guard to the inner base layer, wrapped in order to hide the inner Mutex type.
    async fn get(&self) -> Result<BaseLayerGuard<'_, B>, MonitoredBaseLayerError<B>> {
        self.ensure_operational().await.unwrap();
        Ok(BaseLayerGuard { inner: self.base_layer.lock().await })
    }

    /// Ensures that the inner base layer remains operational (hot-swapping the inner node_url if
    /// needed), and yields the inner base layer.
    /// Note: the monitor has to do a liveness check with the l1 client here, so this has overhead
    /// of an external HTTP call.
    async fn ensure_operational(&self) -> Result<(), MonitoredBaseLayerError<B>> {
        let active_l1_endpoint = self.monitor.get_active_l1_endpoint().await;
        match active_l1_endpoint {
            Ok(new_node_url) if new_node_url != *self.current_node_url.lock().await => {
                info!(
                    "L1 endpoint {} is no longer operational, switching to new operational L1 \
                     endpoint: {}",
                    self.current_node_url.lock().await,
                    &new_node_url
                );

                let mut base_layer = self.base_layer.lock().await;
                base_layer
                    .set_provider_url(new_node_url.clone())
                    .await
                    .map_err(|err| MonitoredBaseLayerError::BaseLayerContractError(err))?;

                *self.current_node_url.lock().await = new_node_url;
            }
            Ok(_) => (), // Noop; the current node URL is still operational.
            Err(L1EndpointMonitorClientError::L1EndpointMonitorError(err)) => Err(err)?,
            Err(L1EndpointMonitorClientError::ClientError(err)) => {
                panic!("Unhandled client error: {err}")
            }
        }

        Ok(())
    }
}

// ensure_operational and delegate to inner base layer.
#[async_trait]
impl<B: BaseLayerContract + Send + Sync> BaseLayerContract for MonitoredBaseLayer<B> {
    type Error = MonitoredBaseLayerError<B>;

    async fn get_proved_block_at(
        &self,
        l1_block: L1BlockNumber,
    ) -> Result<BlockHashAndNumber, Self::Error> {
        self.get()
            .await?
            .get_proved_block_at(l1_block)
            .await
            .map_err(|err| MonitoredBaseLayerError::BaseLayerContractError(err))
    }

    async fn latest_proved_block(
        &self,
        finality: u64,
    ) -> Result<Option<BlockHashAndNumber>, Self::Error> {
        self.get()
            .await?
            .latest_proved_block(finality)
            .await
            .map_err(|err| MonitoredBaseLayerError::BaseLayerContractError(err))
    }

    async fn latest_l1_block_number(
        &self,
        finality: u64,
    ) -> Result<Option<L1BlockNumber>, Self::Error> {
        self.get()
            .await?
            .latest_l1_block_number(finality)
            .await
            .map_err(|err| MonitoredBaseLayerError::BaseLayerContractError(err))
    }

    async fn latest_l1_block(
        &self,
        finality: u64,
    ) -> Result<Option<L1BlockReference>, Self::Error> {
        self.get()
            .await?
            .latest_l1_block(finality)
            .await
            .map_err(|err| MonitoredBaseLayerError::BaseLayerContractError(err))
    }

    async fn l1_block_at(
        &self,
        block_number: L1BlockNumber,
    ) -> Result<Option<L1BlockReference>, Self::Error> {
        self.get()
            .await?
            .l1_block_at(block_number)
            .await
            .map_err(|err| MonitoredBaseLayerError::BaseLayerContractError(err))
    }

    async fn events<'a>(
        &'a self,
        block_range: RangeInclusive<L1BlockNumber>,
        event_identifiers: &'a [&'a str],
    ) -> Result<Vec<L1Event>, Self::Error> {
        self.get()
            .await?
            .events(block_range, event_identifiers)
            .await
            .map_err(|err| MonitoredBaseLayerError::BaseLayerContractError(err))
    }

    async fn get_block_header(
        &self,
        block_number: L1BlockNumber,
    ) -> Result<Option<L1BlockHeader>, Self::Error> {
        self.get()
            .await?
            .get_block_header(block_number)
            .await
            .map_err(|err| MonitoredBaseLayerError::BaseLayerContractError(err))
    }

    async fn set_provider_url(&mut self, url: Url) -> Result<(), Self::Error> {
        self.get()
            .await?
            .set_provider_url(url)
            .await
            .map_err(|err| MonitoredBaseLayerError::BaseLayerContractError(err))
    }
}

impl<B: BaseLayerContract + Send + Sync + std::fmt::Debug> std::fmt::Debug
    for MonitoredBaseLayer<B>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MonitoredBaseLayer")
            .field("current_node_url", &self.current_node_url)
            .field("base_layer", &self.base_layer)
            .finish_non_exhaustive()
    }
}

pub struct BaseLayerGuard<'a, B: BaseLayerContract + Send + Sync> {
    inner: tokio::sync::MutexGuard<'a, B>,
}

impl<B: BaseLayerContract + Send + Sync> Deref for BaseLayerGuard<'_, B> {
    type Target = B;
    fn deref(&self) -> &B {
        &self.inner
    }
}

impl<B: BaseLayerContract + Send + Sync> DerefMut for BaseLayerGuard<'_, B> {
    fn deref_mut(&mut self) -> &mut B {
        &mut self.inner
    }
}

#[derive(thiserror::Error)]
pub enum MonitoredBaseLayerError<B: BaseLayerContract + Send + Sync> {
    #[error("{0}")]
    L1EndpointMonitorError(#[from] L1EndpointMonitorError),
    #[error("{0}")]
    BaseLayerContractError(B::Error),
}

impl<B: BaseLayerContract + Send + Sync> PartialEq for MonitoredBaseLayerError<B> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::L1EndpointMonitorError(left), Self::L1EndpointMonitorError(right)) => {
                left == right
            }
            (Self::BaseLayerContractError(left), Self::BaseLayerContractError(right)) => {
                left == right
            }
            _ => false,
        }
    }
}

impl<B: BaseLayerContract + Send + Sync> std::fmt::Debug for MonitoredBaseLayerError<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MonitoredBaseLayerError::L1EndpointMonitorError(err) => write!(f, "{err:?}"),
            MonitoredBaseLayerError::BaseLayerContractError(err) => write!(f, "{err:?}"),
        }
    }
}
