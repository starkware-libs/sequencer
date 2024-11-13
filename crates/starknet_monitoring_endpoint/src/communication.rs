use starknet_sequencer_infra::component_server::WrapperServer;

use crate::monitoring_endpoint::MonitoringEndpoint;

pub type MonitoringEndpointServer = WrapperServer<MonitoringEndpoint>;
