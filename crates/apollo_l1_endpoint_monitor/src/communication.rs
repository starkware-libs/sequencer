use apollo_infra::component_client::{LocalComponentClient, RemoteComponentClient};
use apollo_infra::component_definitions::{ComponentRequestHandler, RequestWrapper};
use apollo_infra::component_server::{LocalComponentServer, RemoteComponentServer};
use apollo_l1_endpoint_monitor_types::{L1EndpointMonitorRequest, L1EndpointMonitorResponse};
use async_trait::async_trait;
use tracing::instrument;

use crate::monitor::L1EndpointMonitor;

pub type LocalL1EndpointMonitorServer =
    LocalComponentServer<L1EndpointMonitor, L1EndpointMonitorRequest, L1EndpointMonitorResponse>;
pub type RemoteL1EndpointMonitorServer =
    RemoteComponentServer<L1EndpointMonitorRequest, L1EndpointMonitorResponse>;
pub type L1EndpointMonitorRequestWrapper =
    RequestWrapper<L1EndpointMonitorRequest, L1EndpointMonitorResponse>;
pub type LocalL1EndpointMonitorClient =
    LocalComponentClient<L1EndpointMonitorRequest, L1EndpointMonitorResponse>;
pub type RemoteL1EndpointMonitorClient =
    RemoteComponentClient<L1EndpointMonitorRequest, L1EndpointMonitorResponse>;

#[async_trait]
impl ComponentRequestHandler<L1EndpointMonitorRequest, L1EndpointMonitorResponse>
    for L1EndpointMonitor
{
    #[instrument(skip(self))]
    async fn handle_request(
        &mut self,
        request: L1EndpointMonitorRequest,
    ) -> L1EndpointMonitorResponse {
        match request {
            L1EndpointMonitorRequest::GetActiveL1Endpoint() => {
                L1EndpointMonitorResponse::GetActiveL1Endpoint(self.get_active_l1_endpoint().await)
            }
        }
    }
}
