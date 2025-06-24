use std::path::PathBuf;

use const_format::formatcp;
use strum_macros::{Display, EnumString};

use crate::deployment::Deployment;
use crate::deployment_definitions::sepolia_integration::sepolia_integration_hybrid_deployments;
use crate::deployment_definitions::stress_test::stress_test_hybrid_deployments;
use crate::deployment_definitions::testing::system_test_deployments;
use crate::deployment_definitions::testing_env_3::testing_env_3_hybrid_deployments;
use crate::deployment_definitions::upgrade_test::upgrade_test_hybrid_deployments;

#[cfg(test)]
#[path = "deployment_definitions_test.rs"]
mod deployment_definitions_test;

mod sepolia_integration;
mod stress_test;
mod testing;
mod testing_env_3;
mod upgrade_test;

pub(crate) const CONFIG_BASE_DIR: &str = "crates/apollo_deployments/resources/";
pub(crate) const BASE_APP_CONFIG_PATH: &str =
    formatcp!("{}{}", CONFIG_BASE_DIR, "base_app_config.json");
const APP_CONFIGS_DIR_NAME: &str = "app_configs/";

type DeploymentFn = fn() -> Vec<Deployment>;

pub const DEPLOYMENTS: &[DeploymentFn] = &[
    system_test_deployments,
    sepolia_integration_hybrid_deployments,
    upgrade_test_hybrid_deployments,
    testing_env_3_hybrid_deployments,
    stress_test_hybrid_deployments,
];

#[derive(EnumString, Clone, Display, PartialEq, Debug)]
#[strum(serialize_all = "snake_case")]
pub enum Environment {
    Mainnet,
    SepoliaIntegration,
    SepoliaTestnet,
    #[strum(serialize = "stress_test")]
    StressTest,
    Testing,
    #[strum(serialize = "upgrade_test")]
    UpgradeTest,
    #[strum(serialize = "testing_env_3")]
    TestingEnvThree,
}

impl Environment {
    pub fn application_config_dir_path(&self) -> PathBuf {
        PathBuf::from(CONFIG_BASE_DIR).join(self.to_string()).join(APP_CONFIGS_DIR_NAME)
    }
}
