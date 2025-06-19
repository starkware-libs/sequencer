use std::path::PathBuf;

use const_format::formatcp;
use serde_json::{Map, Value};
use starknet_api::block::BlockNumber;
use strum_macros::{Display, EnumString};

use crate::deployment::Deployment;
use crate::deployment_definitions::sepolia_integration::sepolia_integration_hybrid_deployments;
use crate::deployment_definitions::stress_test::stress_test_hybrid_deployments;
use crate::deployment_definitions::testing::system_test_deployments;
use crate::deployment_definitions::testing_env_2::testing_env_2_hybrid_deployments;
use crate::deployment_definitions::testing_env_3::testing_env_3_hybrid_deployments;

#[cfg(test)]
#[path = "deployment_definitions_test.rs"]
mod deployment_definitions_test;

mod sepolia_integration;
mod stress_test;
mod testing;
mod testing_env_2;
mod testing_env_3;

pub(crate) const CONFIG_BASE_DIR: &str = "crates/apollo_deployments/resources/";
pub(crate) const BASE_APP_CONFIG_PATH: &str =
    formatcp!("{}{}", CONFIG_BASE_DIR, "base_app_config.json");
const APP_CONFIGS_DIR_NAME: &str = "app_configs/";

type DeploymentFn = fn() -> Vec<Deployment>;

pub const DEPLOYMENTS: &[DeploymentFn] = &[
    system_test_deployments,
    sepolia_integration_hybrid_deployments,
    testing_env_2_hybrid_deployments,
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
    #[strum(serialize = "testing_env_2")]
    TestingEnvTwo,
    #[strum(serialize = "testing_env_3")]
    TestingEnvThree,
}

impl Environment {
    pub fn application_config_dir_path(&self) -> PathBuf {
        PathBuf::from(CONFIG_BASE_DIR).join(self.to_string()).join(APP_CONFIGS_DIR_NAME)
    }

    pub fn get_l1_provider_config_modifications(&self) -> EnvironmentL1ProviderConfigModifications {
        match self {
            Environment::Testing => EnvironmentL1ProviderConfigModifications::testing(),
            Environment::SepoliaIntegration
            | Environment::TestingEnvTwo
            | Environment::TestingEnvThree
            | Environment::StressTest => {
                EnvironmentL1ProviderConfigModifications::sepolia_integration()
            }
            _ => unimplemented!("This env is not implemented yet"),
        }
    }
}

pub struct EnvironmentL1ProviderConfigModifications {
    pub l1_provider_config_provider_startup_height_override: Option<BlockNumber>,
}

impl EnvironmentL1ProviderConfigModifications {
    pub fn testing() -> Self {
        Self { l1_provider_config_provider_startup_height_override: Some(BlockNumber(1)) }
    }

    pub fn sepolia_integration() -> Self {
        Self { l1_provider_config_provider_startup_height_override: None }
    }

    pub fn as_value(&self) -> Value {
        let mut result = Map::new();
        match self.l1_provider_config_provider_startup_height_override {
            Some(block_number) => {
                let block_number_value = Value::Number(serde_json::Number::from(block_number.0));
                result.insert(
                    "l1_provider_config.provider_startup_height_override".to_string(),
                    block_number_value,
                );
                let is_none_value = Value::Bool(false);
                result.insert(
                    "l1_provider_config.provider_startup_height_override.#is_none".to_string(),
                    is_none_value,
                );
            }
            None => {
                let is_none_value = Value::Bool(true);
                result.insert(
                    "l1_provider_config.provider_startup_height_override.#is_none".to_string(),
                    is_none_value,
                );
            }
        }
        Value::Object(result)
    }
}
