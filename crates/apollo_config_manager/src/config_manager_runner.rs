use apollo_config_manager_types::communication::SharedConfigManagerClient;
use apollo_infra::component_definitions::ComponentStarter;
use apollo_infra::component_server::WrapperServer;

pub struct ConfigManagerRunner {
    // TODO(Nadin): remove dead_code once we have actual config manager runner logic
    #[allow(dead_code)]
    config_manager_client: SharedConfigManagerClient,
    #[allow(dead_code)]
    node_args: Vec<String>,
}

impl ComponentStarter for ConfigManagerRunner {}

impl ConfigManagerRunner {
    pub fn new(config_manager_client: SharedConfigManagerClient, node_args: Vec<String>) -> Self {
        Self { config_manager_client, node_args }
    }
}

pub type ConfigManagerRunnerServer = WrapperServer<ConfigManagerRunner>;
