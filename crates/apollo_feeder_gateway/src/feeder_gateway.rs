use apollo_feeder_gateway_config::config::FeederGatewayConfig;
use apollo_infra::component_definitions::ComponentStarter;
use apollo_infra_utils::type_name::short_type_name;
use async_trait::async_trait;
use tracing::info;

use crate::errors::FeederGatewayRunError;

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

impl FeederGateway {
    pub async fn run(&mut self) -> Result<(), FeederGatewayRunError> {
        info!("FeederGateway run starting.");
        Ok(())
    }
}

#[async_trait]
impl ComponentStarter for FeederGateway {
    async fn start(&mut self) {
        info!("Starting component {}.", short_type_name::<Self>());
        self.run().await.unwrap_or_else(|e| panic!("Failed to start FeederGateway: {e:?}"))
    }
}
