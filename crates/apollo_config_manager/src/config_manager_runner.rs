use std::future::pending;

use apollo_config_manager_config::config::ConfigManagerConfig;
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
    config_manager_config: ConfigManagerConfig,
    config_manager_client: SharedConfigManagerClient,
    latest_node_dynamic_config: NodeDynamicConfig,
    cli_args: Vec<String>,
}

#[async_trait]
impl ComponentStarter for ConfigManagerRunner {
    async fn start(&mut self) {
        default_component_start_fn::<Self>().await;

        info!("ConfigManagerRunner: starting periodic config updates");

        if self.config_manager_config.enable_config_updates {
            // Trigger the periodic config update.
            let mut update_interval = interval(TokioDuration::from_secs_f64(
                self.config_manager_config.config_update_interval_secs,
            ));

            loop {
                update_interval.tick().await;
                if let Err(e) = self.update_config().await {
                    error!("ConfigManagerRunner: failed to update config: {}", e);
                }
            }
        } else {
            // Avoid returning, as this fn is expected to run perpetually.
            pending::<()>().await;
        }
    }
}

impl ConfigManagerRunner {
    pub fn new(
        config_manager_config: ConfigManagerConfig,
        config_manager_client: SharedConfigManagerClient,
        initial_node_dynamic_config: NodeDynamicConfig,
        cli_args: Vec<String>,
    ) -> Self {
        Self {
            config_manager_config,
            config_manager_client,
            latest_node_dynamic_config: initial_node_dynamic_config,
            cli_args,
        }
    }

    // TODO(Nadin): Define a proper result type instead of Box<dyn std::error::Error + Send + Sync>
    pub(crate) async fn update_config(
        &mut self,
    ) -> Result<NodeDynamicConfig, Box<dyn std::error::Error + Send + Sync>> {
        let config = load_and_validate_config(self.cli_args.clone())?;
        let node_dynamic_config = NodeDynamicConfig::from(&config);

        // Compare the previous and the newly read node dynamic config.
        if self.latest_node_dynamic_config == node_dynamic_config {
            // No change, so no action is needed.
            Ok(node_dynamic_config)
        } else {
            // TODO(Nadin/Tsabary): log the diff between the latest and the new node dynamic config.
            // Update the latest node dynamic config.
            self.latest_node_dynamic_config = node_dynamic_config.clone();
            match self
                .config_manager_client
                .set_node_dynamic_config(node_dynamic_config.clone())
                .await
            {
                Ok(()) => {
                    info!("Successfully updated dynamic config");
                    Ok(node_dynamic_config)
                }
                Err(e) => {
                    error!("Failed to update dynamic config: {:?}", e);
                    Err(format!("Failed to update dynamic config: {:?}", e).into())
                }
            }
        }
    }
}

pub type ConfigManagerRunnerServer = WrapperServer<ConfigManagerRunner>;
