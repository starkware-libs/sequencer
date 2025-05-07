use std::path::PathBuf;

use starknet_api::core::ChainId;

use crate::deployment::{
    create_hybrid_instance_config_override,
    ConfigOverride,
    Deployment,
    DeploymentConfigOverride,
};
use crate::deployment_definitions::{Environment, BASE_APP_CONFIG_PATH};
use crate::service::{DeploymentName, ExternalSecret};

const TESTING_ENV_2_DEPLOYMENT_CONFIG_OVERRIDE: DeploymentConfigOverride =
    DeploymentConfigOverride::new(
        "0xA43812F9C610851daF67c5FA36606Ea8c8Fa7caE",
        "SN_GOERLI",
        "0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7",
        "https://fgw-sn-test-sepolia-2-sepolia.gateway-proxy.sw-dev.io",
        "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d",
    );

const FIRST_NODE_NAMESPACE: &str = "sequencer-test-sepolia-0";

fn testing_env_2_node_0_config_override() -> ConfigOverride {
    ConfigOverride::new(
        TESTING_ENV_2_DEPLOYMENT_CONFIG_OVERRIDE,
        create_hybrid_instance_config_override(1, FIRST_NODE_NAMESPACE),
    )
}
fn testing_env_2_node_1_config_override() -> ConfigOverride {
    ConfigOverride::new(
        TESTING_ENV_2_DEPLOYMENT_CONFIG_OVERRIDE,
        create_hybrid_instance_config_override(2, FIRST_NODE_NAMESPACE),
    )
}
fn testing_env_2_node_2_config_override() -> ConfigOverride {
    ConfigOverride::new(
        TESTING_ENV_2_DEPLOYMENT_CONFIG_OVERRIDE,
        create_hybrid_instance_config_override(3, FIRST_NODE_NAMESPACE),
    )
}
fn testing_env_2_node_3_config_override() -> ConfigOverride {
    ConfigOverride::new(
        TESTING_ENV_2_DEPLOYMENT_CONFIG_OVERRIDE,
        create_hybrid_instance_config_override(4, FIRST_NODE_NAMESPACE),
    )
}

const TESTING_ENV_2_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME: &str =
    "sn-test-sepolia-2-sepolia.gateway-proxy.sw-dev.io";
const TESTING_ENV_2_INGRESS_DOMAIN: &str = "sw-dev.io";

pub(crate) fn testing_env_2_hybrid_deployment_node_0() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::TestingEnvTwo,
        "integration_hybrid_node_0",
        Some(ExternalSecret::new("sequencer-test-sepolia-0")),
        PathBuf::from(BASE_APP_CONFIG_PATH),
        testing_env_2_node_0_config_override(),
        TESTING_ENV_2_INGRESS_DOMAIN.to_string(),
        Some(vec![TESTING_ENV_2_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
    )
}

pub(crate) fn testing_env_2_hybrid_deployment_node_1() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::TestingEnvTwo,
        "integration_hybrid_node_1",
        Some(ExternalSecret::new("sequencer-test-sepolia-1")),
        PathBuf::from(BASE_APP_CONFIG_PATH),
        testing_env_2_node_1_config_override(),
        TESTING_ENV_2_INGRESS_DOMAIN.to_string(),
        Some(vec![TESTING_ENV_2_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
    )
}

pub(crate) fn testing_env_2_hybrid_deployment_node_2() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::TestingEnvTwo,
        "integration_hybrid_node_2",
        Some(ExternalSecret::new("sequencer-test-sepolia-2")),
        PathBuf::from(BASE_APP_CONFIG_PATH),
        testing_env_2_node_2_config_override(),
        TESTING_ENV_2_INGRESS_DOMAIN.to_string(),
        Some(vec![TESTING_ENV_2_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
    )
}

pub(crate) fn testing_env_2_hybrid_deployment_node_3() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::TestingEnvTwo,
        "integration_hybrid_node_3",
        Some(ExternalSecret::new("sequencer-test-sepolia-3")),
        PathBuf::from(BASE_APP_CONFIG_PATH),
        testing_env_2_node_3_config_override(),
        TESTING_ENV_2_INGRESS_DOMAIN.to_string(),
        Some(vec![TESTING_ENV_2_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
    )
}
