use apollo_config_manager_types::communication::SharedConfigManagerClient;
use apollo_infra::component_definitions::ComponentStarter;
use apollo_infra::component_server::WrapperServer;
use apollo_node_config::config_utils::load_and_validate_config;
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
    cli_args: Vec<String>,
}

#[async_trait]
impl ComponentStarter for ConfigManagerRunner {
    async fn start(&mut self) {
        info!("Starting ConfigManagerRunner");

        // TODO(Nadin): make this configurable
        let mut update_interval = interval(TokioDuration::from_secs(60));

        loop {
            update_interval.tick().await;
            self.update_config().await;
        }
    }
}

impl ConfigManagerRunner {
    pub fn new(config_manager_client: SharedConfigManagerClient, cli_args: Vec<String>) -> Self {
        Self { config_manager_client, cli_args }
    }

    async fn update_config(&self) {
        // Update consensus config
        if let Err(e) = self.update_consensus_config().await {
            error!("Failed to update consensus config: {}", e);
        }
    }

    pub async fn update_consensus_config(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Loading and validating config");

        // Load and validate the config using the CLI arguments
        let config = load_and_validate_config(self.cli_args.clone())?;

        // Extract consensus dynamic config if consensus manager config exists
        if let Some(consensus_manager_config) = &config.consensus_manager_config {
            let consensus_dynamic_config =
                &consensus_manager_config.consensus_manager_config.dynamic_config;

            info!("Built consensus dynamic config: {:?}", consensus_dynamic_config);

            // Send the new config to the config manager through the client
            match self
                .config_manager_client
                .set_consensus_dynamic_config(consensus_dynamic_config.clone())
                .await
            {
                Ok(()) => {
                    info!(
                        "Successfully sent consensus dynamic config to config manager: {:?}",
                        consensus_dynamic_config
                    );
                }
                Err(e) => {
                    error!("Failed to send consensus dynamic config to config manager: {:?}", e);
                    return Err(format!("Failed to send config to config manager: {:?}", e).into());
                }
            }

            Ok(())
        } else {
            info!("No consensus manager config found, skipping consensus config update");
            Ok(())
        }
    }
}

pub type ConfigManagerRunnerServer = WrapperServer<ConfigManagerRunner>;
