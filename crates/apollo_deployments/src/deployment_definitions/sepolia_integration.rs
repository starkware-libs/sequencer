use std::path::PathBuf;

use starknet_api::core::ChainId;

use crate::deployment::{
    create_hybrid_instance_config_override,
    ConfigOverride,
    Deployment,
    DeploymentConfigOverride,
};
use crate::deployment_definitions::{Environment, BASE_APP_CONFIG_PATH};
use crate::service::{DeploymentName, ExternalSecret, IngressParams};

fn sepolia_integration_deployment_config_override() -> DeploymentConfigOverride {
    DeploymentConfigOverride::new(
        "0x4737c0c1B4D5b1A687B42610DdabEE781152359c",
        "SN_INTEGRATION_SEPOLIA",
        "0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7",
        "https://feeder.integration-sepolia.starknet.io/",
        "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d",
    )
}

const FIRST_NODE_NAMESPACE: &str = "apollo-sepolia-integration-0";

fn sepolia_integration_node_0_config_override() -> ConfigOverride {
    ConfigOverride::new(
        sepolia_integration_deployment_config_override(),
        create_hybrid_instance_config_override(0, FIRST_NODE_NAMESPACE),
    )
}
fn sepolia_integration_node_1_config_override() -> ConfigOverride {
    ConfigOverride::new(
        sepolia_integration_deployment_config_override(),
        create_hybrid_instance_config_override(1, FIRST_NODE_NAMESPACE),
    )
}
fn sepolia_integration_node_2_config_override() -> ConfigOverride {
    ConfigOverride::new(
        sepolia_integration_deployment_config_override(),
        create_hybrid_instance_config_override(2, FIRST_NODE_NAMESPACE),
    )
}
fn sepolia_integration_node_3_config_override() -> ConfigOverride {
    ConfigOverride::new(
        sepolia_integration_deployment_config_override(),
        create_hybrid_instance_config_override(3, FIRST_NODE_NAMESPACE),
    )
}

fn get_ingress_params() -> IngressParams {
    IngressParams::new(
        SEPOLIA_INTEGRATION_INGRESS_DOMAIN.to_string(),
        Some(vec![SEPOLIA_INTEGRATION_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
    )
}

const SEPOLIA_INTEGRATION_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME: &str =
    "integration-sepolia.starknet.io";

const SEPOLIA_INTEGRATION_INGRESS_DOMAIN: &str = "starknet.io";

// Integration deployments

pub(crate) fn sepolia_integration_hybrid_deployment_node_0() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::SepoliaIntegration,
        "integration_hybrid_node_0",
        Some(ExternalSecret::new("apollo-sepolia-integration-0")),
        PathBuf::from(BASE_APP_CONFIG_PATH),
        sepolia_integration_node_0_config_override(),
        get_ingress_params(),
    )
}

pub(crate) fn sepolia_integration_hybrid_deployment_node_1() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::SepoliaIntegration,
        "integration_hybrid_node_1",
        Some(ExternalSecret::new("apollo-sepolia-integration-1")),
        PathBuf::from(BASE_APP_CONFIG_PATH),
        sepolia_integration_node_1_config_override(),
        get_ingress_params(),
    )
}

pub(crate) fn sepolia_integration_hybrid_deployment_node_2() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::SepoliaIntegration,
        "integration_hybrid_node_2",
        Some(ExternalSecret::new("apollo-sepolia-integration-2")),
        PathBuf::from(BASE_APP_CONFIG_PATH),
        sepolia_integration_node_2_config_override(),
        get_ingress_params(),
    )
}

pub(crate) fn sepolia_integration_hybrid_deployment_node_3() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::SepoliaIntegration,
        "integration_hybrid_node_3",
        Some(ExternalSecret::new("apollo-sepolia-integration-3")),
        PathBuf::from(BASE_APP_CONFIG_PATH),
        sepolia_integration_node_3_config_override(),
        get_ingress_params(),
    )
}
