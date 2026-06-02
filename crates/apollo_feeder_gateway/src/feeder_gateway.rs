use apollo_feeder_gateway_config::config::FeederGatewayConfig;

pub struct FeederGateway {
    pub config: FeederGatewayConfig,
}

impl FeederGateway {
    pub fn new(config: FeederGatewayConfig) -> Self {
        Self { config }
    }
}

pub fn create_feeder_gateway(config: FeederGatewayConfig) -> FeederGateway {
    FeederGateway::new(config)
}
