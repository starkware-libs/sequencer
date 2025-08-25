use apollo_infra::component_definitions::ComponentStarter;
use apollo_infra::component_server::WrapperServer;

use crate::config::ConfigManagerConfig;

pub struct ConfigManagerRunner {}

impl ComponentStarter for ConfigManagerRunner {}

impl ConfigManagerRunner {
    pub fn new(_config: ConfigManagerConfig) -> Self {
        Self {}
    }
}

pub type ConfigManagerRunnerServer = WrapperServer<ConfigManagerRunner>;
