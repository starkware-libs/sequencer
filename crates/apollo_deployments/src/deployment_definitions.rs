use std::path::PathBuf;

use apollo_infra::component_definitions::{LocalServerConfig, RemoteClientConfig};
use starknet_api::core::ChainId;
use strum_macros::{Display, EnumString};

use crate::deployment::{
    Deployment,
    DeploymentAndPreset,
    DEPLOYMENT_IMAGE_FOR_PRE_INTEGRATION,
    DEPLOYMENT_IMAGE_FOR_TESTING,
};
use crate::service::{DeploymentName, ExternalSecret};

#[cfg(test)]
#[path = "deployment_definitions_test.rs"]
mod deployment_definitions_test;

// TODO(Tsabary): separate deployments to different modules.

const SYSTEM_TEST_BASE_APP_CONFIG_PATH: &str =
    "config/sequencer/testing/base_app_configs/single_node_deployment_test.json";

const INTEGRATION_BASE_APP_CONFIG_PATH_NODE_0: &str =
    "config/sequencer/sepolia_integration/base_app_configs/node_0.json";
const INTEGRATION_BASE_APP_CONFIG_PATH_NODE_1: &str =
    "config/sequencer/sepolia_integration/base_app_configs/node_1.json";
const INTEGRATION_BASE_APP_CONFIG_PATH_NODE_2: &str =
    "config/sequencer/sepolia_integration/base_app_configs/node_2.json";

pub(crate) const CONFIG_BASE_DIR: &str = "config/sequencer/";
const APP_CONFIGS_DIR_NAME: &str = "app_configs/";

type DeploymentFn = fn() -> DeploymentAndPreset;

// TODO(Tsabary): create deployment instances per per deployment.

pub const DEPLOYMENTS: &[DeploymentFn] = &[
    system_test_distributed_deployment,
    system_test_consolidated_deployment,
    integration_consolidated_deployment,
    integration_hybrid_deployment_node_0,
    integration_hybrid_deployment_node_1,
    integration_hybrid_deployment_node_2,
];

// Integration deployments

fn integration_hybrid_deployment_node_0() -> DeploymentAndPreset {
    DeploymentAndPreset::new(Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::SepoliaIntegration,
        "integration_hybrid_node_0",
        Some(ExternalSecret::new("node-0-integration-secrets")),
        DEPLOYMENT_IMAGE_FOR_PRE_INTEGRATION,
        PathBuf::from(INTEGRATION_BASE_APP_CONFIG_PATH_NODE_0),
        vec![],
    ))
}

fn integration_hybrid_deployment_node_1() -> DeploymentAndPreset {
    DeploymentAndPreset::new(Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::SepoliaIntegration,
        "integration_hybrid_node_1",
        Some(ExternalSecret::new("node-1-integration-secrets")),
        DEPLOYMENT_IMAGE_FOR_PRE_INTEGRATION,
        PathBuf::from(INTEGRATION_BASE_APP_CONFIG_PATH_NODE_1),
        vec![],
    ))
}

fn integration_hybrid_deployment_node_2() -> DeploymentAndPreset {
    DeploymentAndPreset::new(Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::SepoliaIntegration,
        "integration_hybrid_node_2",
        Some(ExternalSecret::new("node-2-integration-secrets")),
        DEPLOYMENT_IMAGE_FOR_PRE_INTEGRATION,
        PathBuf::from(INTEGRATION_BASE_APP_CONFIG_PATH_NODE_2),
        vec![],
    ))
}

fn integration_consolidated_deployment() -> DeploymentAndPreset {
    DeploymentAndPreset::new(Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::ConsolidatedNode,
        Environment::SepoliaIntegration,
        "integration_consolidated",
        Some(ExternalSecret::new("node-1-integration-secrets")),
        DEPLOYMENT_IMAGE_FOR_PRE_INTEGRATION,
        PathBuf::from(INTEGRATION_BASE_APP_CONFIG_PATH_NODE_0),
        vec![],
    ))
}

// System test deployments
fn system_test_distributed_deployment() -> DeploymentAndPreset {
    DeploymentAndPreset::new(Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::DistributedNode,
        Environment::Testing,
        "deployment_test_distributed",
        None,
        DEPLOYMENT_IMAGE_FOR_TESTING,
        PathBuf::from(SYSTEM_TEST_BASE_APP_CONFIG_PATH),
        vec![],
    ))
}

fn system_test_consolidated_deployment() -> DeploymentAndPreset {
    DeploymentAndPreset::new(Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::ConsolidatedNode,
        Environment::Testing,
        "deployment_test_consolidated",
        None,
        DEPLOYMENT_IMAGE_FOR_TESTING,
        PathBuf::from(SYSTEM_TEST_BASE_APP_CONFIG_PATH),
        vec![],
    ))
}

#[derive(EnumString, Clone, Display, PartialEq, Debug)]
#[strum(serialize_all = "snake_case")]
pub enum Environment {
    Testing,
    SepoliaIntegration,
    SepoliaTestnet,
    Mainnet,
}

impl Environment {
    pub fn application_config_dir_path(&self) -> PathBuf {
        PathBuf::from(CONFIG_BASE_DIR).join(self.to_string()).join(APP_CONFIGS_DIR_NAME)
    }

    pub fn get_component_config_modifications(&self) -> EnvironmentComponentConfigModifications {
        match self {
            Environment::Testing => EnvironmentComponentConfigModifications::testing(),
            Environment::SepoliaIntegration => {
                EnvironmentComponentConfigModifications::sepolia_integration()
            }
            Environment::SepoliaTestnet => unimplemented!("SepoliaTestnet is not implemented yet"),
            Environment::Mainnet => unimplemented!("Mainnet is not implemented yet"),
        }
    }
}

pub struct EnvironmentComponentConfigModifications {
    pub local_server_config: LocalServerConfig,
    pub max_concurrency: usize,
    pub remote_client_config: RemoteClientConfig,
}

impl EnvironmentComponentConfigModifications {
    pub fn testing() -> Self {
        Self {
            local_server_config: LocalServerConfig { channel_buffer_size: 32 },
            max_concurrency: 10,
            remote_client_config: RemoteClientConfig {
                retries: 3,
                idle_connections: 5,
                idle_timeout: 90,
                retry_interval: 3,
            },
        }
    }

    pub fn sepolia_integration() -> Self {
        Self {
            local_server_config: LocalServerConfig { channel_buffer_size: 128 },
            max_concurrency: 100,
            remote_client_config: RemoteClientConfig {
                retries: 3,
                idle_connections: usize::MAX,
                idle_timeout: 1,
                retry_interval: 1,
            },
        }
    }
}
