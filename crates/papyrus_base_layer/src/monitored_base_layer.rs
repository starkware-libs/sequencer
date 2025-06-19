use apollo_l1_endpoint_monitor_types::SharedL1EndpointMonitorClient;
use tokio::sync::Mutex;
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
    // Ensures that the inner base layer remains operational (hot-swapping the inner node_url if
    // needed), and yields the inner base layer.
    pub async fn ensure_operational(&self) -> Result<(), MonitoredBaseLayerError> {
        todo!(
            "Asks the monitor what the current active L1 endpoint is (that is synced with the \
             rest of the base layers), and modifies the inner base layer to use it if it's using \
             a different one. "
        )
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

#[derive(Debug, thiserror::Error)]
pub enum MonitoredBaseLayerError {}
