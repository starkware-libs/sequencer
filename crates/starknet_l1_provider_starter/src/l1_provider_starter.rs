use std::any::type_name;

use async_trait::async_trait;
use starknet_l1_provider_types::communication::SharedL1ProviderClient;
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use starknet_sequencer_infra::errors::ComponentError;
use tokio::time;
use tracing::info;

use crate::config::L1ProviderStarterConfig;
use crate::errors::L1ProviderRunError;

pub struct L1ProviderStarter {
    pub config: L1ProviderStarterConfig,
    pub l1_provider_client: SharedL1ProviderClient,
}

impl L1ProviderStarter {
    pub fn new(
        config: L1ProviderStarterConfig,
        l1_provider_client: SharedL1ProviderClient,
    ) -> Self {
        L1ProviderStarter { config, l1_provider_client }
    }

    pub async fn run(&mut self) -> Result<(), L1ProviderRunError> {
        let mut ticker = time::interval(self.config.interval);

        loop {
            ticker.tick().await;
            if let Err(e) = self.l1_provider_client.start().await {
                return Err(L1ProviderRunError::L1ProviderClientError(format!(
                    "Failed to call start: {e}"
                )));
            }
        }
    }
}

#[async_trait]
impl ComponentStarter for L1ProviderStarter {
    async fn start(&mut self) -> Result<(), ComponentError> {
        info!("Starting component {}.", type_name::<Self>());
        self.run().await.map_err(|_| ComponentError::InternalComponentError)
    }
}

pub fn create_l1_provider_start_er(
    config: L1ProviderStarterConfig,
    l1_provider_client: SharedL1ProviderClient,
) -> L1ProviderStarter {
    L1ProviderStarter::new(config, l1_provider_client)
}
