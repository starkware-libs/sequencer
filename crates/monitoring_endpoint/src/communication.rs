use starknet_sequencer_infra::component_server::{create_empty_server, WrapperServer};

use crate::monitoring_endpoint::MonitoringEndpoint;

pub type MonitoringEndpointServer = WrapperServer<MonitoringEndpoint>;

pub fn create_monitoring_endpoint_server(
    monitoring_endpoint: MonitoringEndpoint,
) -> MonitoringEndpointServer {
    create_empty_server(monitoring_endpoint)
}
