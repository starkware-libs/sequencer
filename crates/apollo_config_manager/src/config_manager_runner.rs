use apollo_config_manager_types::communication::SharedConfigManagerClient;
use apollo_infra::component_definitions::ComponentStarter;
use apollo_infra::component_server::WrapperServer;
use async_trait::async_trait;
use tracing::info;

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
        info!("Starting ConfigManagerRunner");
        // TODO: Implement configuration loading once node configuration module is available.
        info!(
            "ConfigManagerRunner start logic placeholder – configuration loading is not yet \
             implemented"
        );
        // TODO(Nadin): Use the config to initialize dynamic config management
    }
}

impl ConfigManagerRunner {
    pub fn new(config_manager_client: SharedConfigManagerClient, cli_args: Vec<String>) -> Self {
        Self { config_manager_client, cli_args }
    }
}

pub type ConfigManagerRunnerServer = WrapperServer<ConfigManagerRunner>;
