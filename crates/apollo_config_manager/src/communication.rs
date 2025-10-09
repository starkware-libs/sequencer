use apollo_config_manager_types::communication::{ConfigManagerRequest, ConfigManagerResponse};
use apollo_infra::component_server::{ConcurrentLocalComponentServer, WrapperServer};

use crate::config_manager::ConfigManager;
use crate::config_manager_runner::ConfigManagerRunner;

pub type LocalConfigManagerServer =
    ConcurrentLocalComponentServer<ConfigManager, ConfigManagerRequest, ConfigManagerResponse>;

pub type ConfigManagerRunnerServer = WrapperServer<ConfigManagerRunner>;
