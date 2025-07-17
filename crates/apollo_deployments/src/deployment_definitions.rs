use std::fmt::{Display, Formatter, Result};
use std::path::PathBuf;

use serde::Serialize;
use strum_macros::{Display, EnumString};

use crate::deployment::Deployment;
use crate::deployment_definitions::mainnet::mainnet_hybrid_deployments;
use crate::deployment_definitions::potc2_sepolia::potc2_sepolia_hybrid_deployments;
use crate::deployment_definitions::sepolia_integration::sepolia_integration_hybrid_deployments;
use crate::deployment_definitions::sepolia_testnet::sepolia_testnet_hybrid_deployments;
use crate::deployment_definitions::stress_test::stress_test_hybrid_deployments;
use crate::deployment_definitions::testing::system_test_deployments;
use crate::deployment_definitions::upgrade_test::upgrade_test_hybrid_deployments;

#[cfg(test)]
#[path = "deployment_definitions_test.rs"]
mod deployment_definitions_test;

mod mainnet;
mod potc2_sepolia;
mod sepolia_integration;
mod sepolia_testnet;
mod stress_test;
mod testing;
mod upgrade_test;

pub(crate) const CONFIG_BASE_DIR: &str = "crates/apollo_deployments/resources/";
pub(crate) const DEPLOYMENT_CONFIG_DIR_NAME: &str = "deployments/";
pub(crate) const BASE_APP_CONFIG_PATH: &str =
    "crates/apollo_deployments/resources/base_app_config.json";

type DeploymentFn = fn() -> Vec<Deployment>;

pub const DEPLOYMENTS: &[DeploymentFn] = &[
    potc2_sepolia_hybrid_deployments,
    mainnet_hybrid_deployments,
    sepolia_integration_hybrid_deployments,
    sepolia_testnet_hybrid_deployments,
    stress_test_hybrid_deployments,
    system_test_deployments,
    upgrade_test_hybrid_deployments,
];

#[derive(Clone, Debug, PartialEq)]
pub enum Environment {
    CloudK8s(CloudK8sEnvironment),
    LocalK8s,
}

impl Display for Environment {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Environment::CloudK8s(e) => write!(f, "{e}"),
            Environment::LocalK8s => write!(f, "testing"),
        }
    }
}

#[derive(EnumString, Clone, Display, PartialEq, Debug)]
#[strum(serialize_all = "snake_case")]
pub enum CloudK8sEnvironment {
    Potc2,
    Mainnet,
    SepoliaIntegration,
    SepoliaTestnet,
    #[strum(serialize = "stress_test")]
    StressTest,
    #[strum(serialize = "upgrade_test")]
    UpgradeTest,
}

impl Environment {
    pub fn env_dir_path(&self) -> PathBuf {
        let env_str = match self {
            Environment::CloudK8s(env) => env.to_string(),
            Environment::LocalK8s => "testing".to_string(),
        };
        PathBuf::from(CONFIG_BASE_DIR).join(DEPLOYMENT_CONFIG_DIR_NAME).join(env_str)
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct StateSyncConfig {
    #[serde(rename = "state_sync_config.central_sync_client_config.#is_none")]
    state_sync_config_central_sync_client_config_is_none: bool,
    #[serde(rename = "state_sync_config.p2p_sync_client_config.#is_none")]
    state_sync_config_p2p_sync_client_config_is_none: bool,
    #[serde(rename = "state_sync_config.network_config.#is_none")]
    state_sync_config_network_config_is_none: bool,
}

pub enum StateSyncType {
    Central,
    P2P,
}

impl StateSyncType {
    pub fn get_state_sync_config(&self) -> StateSyncConfig {
        match self {
            StateSyncType::Central => StateSyncConfig {
                state_sync_config_central_sync_client_config_is_none: false,
                state_sync_config_p2p_sync_client_config_is_none: true,
                state_sync_config_network_config_is_none: true,
            },
            StateSyncType::P2P => StateSyncConfig {
                state_sync_config_central_sync_client_config_is_none: true,
                state_sync_config_p2p_sync_client_config_is_none: false,
                state_sync_config_network_config_is_none: false,
            },
        }
    }
}

#[derive(Clone, Debug, Display, Serialize, PartialEq)]
pub enum ServicePort {
    Batcher,
    ClassManager,
    Gateway,
    L1EndpointMonitor,
    L1GasPriceProvider,
    L1Provider,
    Mempool,
    MempoolP2p,
    SierraCompiler,
    StateSync,
    HttpServer,
    MonitoringEndpoint,
}
