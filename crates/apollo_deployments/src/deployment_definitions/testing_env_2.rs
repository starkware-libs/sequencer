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

const TESTING_ENV_2_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME: &str =
    "sn-test-sepolia-2-sepolia.gateway-proxy.sw-dev.io";
const TESTING_ENV_2_INGRESS_DOMAIN: &str = "sw-dev.io";
const FIRST_NODE_NAMESPACE: &str = "sequencer-test-sepolia-0";

pub(crate) fn testing_env_2_hybrid_deployments() -> Vec<Deployment> {
    vec![
        testing_env_2_hybrid_deployment_node_0(),
        testing_env_2_hybrid_deployment_node_1(),
        testing_env_2_hybrid_deployment_node_2(),
        testing_env_2_hybrid_deployment_node_3(),
    ]
}

fn testing_env_2_deployment_config_override() -> DeploymentConfigOverride {
    DeploymentConfigOverride::new(
        "0xA43812F9C610851daF67c5FA36606Ea8c8Fa7caE",
        "SN_GOERLI",
        "0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7",
        "https://fgw-sn-test-sepolia-2-sepolia.gateway-proxy.sw-dev.io",
        "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d",
    )
}

fn testing_env_2_config_override(id: usize) -> ConfigOverride {
    ConfigOverride::new(
        testing_env_2_deployment_config_override(),
        create_hybrid_instance_config_override(id, FIRST_NODE_NAMESPACE),
    )
}

fn get_ingress_params() -> IngressParams {
    IngressParams::new(
        TESTING_ENV_2_INGRESS_DOMAIN.to_string(),
        Some(vec![TESTING_ENV_2_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
    )
}

fn testing_env_2_hybrid_deployment_node_0() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::TestingEnvTwo,
        "integration_hybrid_node_0",
        Some(ExternalSecret::new("sequencer-test-sepolia-0")),
        PathBuf::from(BASE_APP_CONFIG_PATH),
        testing_env_2_config_override(0),
        get_ingress_params(),
    )
}

fn testing_env_2_hybrid_deployment_node_1() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::TestingEnvTwo,
        "integration_hybrid_node_1",
        Some(ExternalSecret::new("sequencer-test-sepolia-1")),
        PathBuf::from(BASE_APP_CONFIG_PATH),
        testing_env_2_config_override(1),
        get_ingress_params(),
    )
}

fn testing_env_2_hybrid_deployment_node_2() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::TestingEnvTwo,
        "integration_hybrid_node_2",
        Some(ExternalSecret::new("sequencer-test-sepolia-2")),
        PathBuf::from(BASE_APP_CONFIG_PATH),
        testing_env_2_config_override(2),
        get_ingress_params(),
    )
}

fn testing_env_2_hybrid_deployment_node_3() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::TestingEnvTwo,
        "integration_hybrid_node_3",
        Some(ExternalSecret::new("sequencer-test-sepolia-3")),
        PathBuf::from(BASE_APP_CONFIG_PATH),
        testing_env_2_config_override(3),
        get_ingress_params(),
    )
}
