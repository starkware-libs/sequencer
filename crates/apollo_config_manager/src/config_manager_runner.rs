use apollo_config_manager_types::communication::SharedConfigManagerClient;
use apollo_infra::component_definitions::{default_component_start_fn, ComponentStarter};
use apollo_infra::component_server::WrapperServer;
use apollo_node_config::config_utils::load_and_validate_config;
use apollo_node_config::node_config::NodeDynamicConfig;
use async_trait::async_trait;
use tokio::time::{interval, Duration as TokioDuration};
use tracing::{error, info};

#[cfg(test)]
#[path = "config_manager_runner_tests.rs"]
pub mod config_manager_runner_tests;

pub struct ConfigManagerRunner {
    // TODO(Nadin): remove dead_code once we have actual config manager runner logic
    #[allow(dead_code)]
    config_manager_client: SharedConfigManagerClient,
    #[allow(dead_code)]
    cli_args: Vec<String>,
}

#[async_trait]
impl ComponentStarter for ConfigManagerRunner {
    async fn start(&mut self) {
        default_component_start_fn::<Self>().await;

        info!("ConfigManagerRunner: starting periodic config updates");

        // TODO(Nadin): make this configurable
        let mut update_interval = interval(TokioDuration::from_secs(60));

        loop {
            update_interval.tick().await;
            if let Err(e) = self.update_config().await {
                error!("ConfigManagerRunner: failed to update config: {}", e);
            }
        }
    }
}

impl ConfigManagerRunner {
    pub fn new(config_manager_client: SharedConfigManagerClient, cli_args: Vec<String>) -> Self {
        Self { config_manager_client, cli_args }
    }

    // TODO(Nadin): Define a proper result type instead of Box<dyn std::error::Error + Send + Sync>
    pub(crate) async fn update_config(
        &self,
    ) -> Result<NodeDynamicConfig, Box<dyn std::error::Error + Send + Sync>> {
        let config = load_and_validate_config(self.cli_args.clone())?;

        // Extract consensus dynamic config from the loaded config
        let consensus_manager_config = config
            .consensus_manager_config
            .as_ref()
            .expect("consensus_manager_config must be present");

        let node_dynamic_config = NodeDynamicConfig {
            consensus_dynamic_config: consensus_manager_config
                .consensus_manager_config
                .dynamic_config
                .clone(),
        };

        info!("Extracted NodeDynamicConfig: {:?}", node_dynamic_config);

        // TODO(Nadin): Send the new config to the config manager through the client.

        Ok(node_dynamic_config)
    }
}

pub type ConfigManagerRunnerServer = WrapperServer<ConfigManagerRunner>;
