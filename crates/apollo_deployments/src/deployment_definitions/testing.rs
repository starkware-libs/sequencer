use std::path::PathBuf;

use starknet_api::core::ChainId;

use crate::deployment::{
    ConfigOverride,
    Deployment,
    DeploymentConfigOverride,
    InstanceConfigOverride,
};
use crate::deployment_definitions::{Environment, BASE_APP_CONFIG_PATH};
use crate::service::DeploymentName;

fn testing_deployment_config_override() -> DeploymentConfigOverride {
    DeploymentConfigOverride::new(
        "0x5FbDB2315678afecb367f032d93F642f64180aa3",
        "CHAIN_ID_SUBDIR",
        "0x1001",
        "https://integration-sepolia.starknet.io/",
        "0x1002",
    )
}

fn testing_instance_config_override() -> InstanceConfigOverride {
    InstanceConfigOverride::new(
        "",
        true,
        "0x0101010101010101010101010101010101010101010101010101010101010101",
        "",
        true,
        "0x0101010101010101010101010101010101010101010101010101010101010101",
        "0x64",
    )
}

fn testing_config_override() -> ConfigOverride {
    ConfigOverride::new(testing_deployment_config_override(), testing_instance_config_override())
}

const TESTING_INGRESS_DOMAIN: &str = "sw-dev.io";

pub(crate) fn system_test_distributed_deployment() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::DistributedNode,
        Environment::Testing,
        "deployment_test_distributed",
        None,
        PathBuf::from(BASE_APP_CONFIG_PATH),
        testing_config_override(),
        TESTING_INGRESS_DOMAIN.to_string(),
        None,
    )
}

pub(crate) fn system_test_hybrid_deployment() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::Testing,
        "deployment_test_hybrid",
        None,
        PathBuf::from(BASE_APP_CONFIG_PATH),
        testing_config_override(),
        TESTING_INGRESS_DOMAIN.to_string(),
        None,
    )
}

pub(crate) fn system_test_consolidated_deployment() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::ConsolidatedNode,
        Environment::Testing,
        "deployment_test_consolidated",
        None,
        PathBuf::from(BASE_APP_CONFIG_PATH),
        testing_config_override(),
        TESTING_INGRESS_DOMAIN.to_string(),
        None,
    )
}
