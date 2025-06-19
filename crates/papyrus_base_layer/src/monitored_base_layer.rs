use apollo_l1_endpoint_monitor_types::{
    L1EndpointMonitorClientError,
    L1EndpointMonitorError,
    SharedL1EndpointMonitorClient,
};
use tokio::sync::Mutex;
use tracing::{error, info};
use url::Url;

use crate::BaseLayerContract;

// Using interior mutability for modifiable fields in order to comply with the base layer's
// largely immutable API.
pub struct MonitoredBaseLayer<B: BaseLayerContract + Send + Sync> {
    pub monitor: SharedL1EndpointMonitorClient,
    current_node_url: Mutex<Url>,
    base_layer: Mutex<B>,
}

impl<B: BaseLayerContract + Send + Sync> MonitoredBaseLayer<B> {
    /// Ensures that the inner base layer remains operational (hot-swapping the inner node_url if
    /// needed), and yields the inner base layer.
    /// Note: the monitor has to do a liveness check with the l1 client here, so this has overhead
    /// of an external HTTP call.
    async fn _ensure_operational(&self) -> Result<(), MonitoredBaseLayerError<B>> {
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

// TODO(Gilad): implement BaseLayerContract for MonitoredBaseLayer so it'll be a proper wrapper for
// the inner base_layer.

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
            MonitoredBaseLayerError::L1EndpointMonitorError(err) => write!(f, "{:?}", err),
            MonitoredBaseLayerError::BaseLayerContractError(err) => write!(f, "{:?}", err),
        }
    }
}
