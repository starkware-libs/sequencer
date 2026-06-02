use apollo_infra::component_server::WrapperServer;

use crate::feeder_gateway::FeederGateway as FeederGatewayComponent;

pub type FeederGateway = WrapperServer<FeederGatewayComponent>;
