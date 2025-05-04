use apollo_l1_endpoint_monitor_types::{L1EndpointMonitorResult, L1EndpointOperationalStatus};
use url::Url;

pub struct L1EndpointMonitor;

impl L1EndpointMonitor {
    pub async fn ensure_operational(
        &mut self,
        _node_url: Url,
    ) -> L1EndpointMonitorResult<L1EndpointOperationalStatus> {
        // TODO(Gilad): will be added soon.
        Ok(L1EndpointOperationalStatus::Operational)
    }
}
