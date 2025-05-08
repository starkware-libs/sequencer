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

fn testing_env_3_deployment_config_override() -> DeploymentConfigOverride {
    DeploymentConfigOverride::new(
        "0xa23a6BA7DA61988D2420dAE9F10eE964552459d5",
        "SN_GOERLI",
        "0x7c07a3eec8ff611328722c3fc3e5d2e4ef2f60740c0bf86c756606036b74c16",
        "https://fgw-sn-test-sepolia-3-sepolia.gateway-proxy.sw-dev.io",
        "0x54a93d918d62b2fb62b25e77d9cb693bd277ab7e6fa236e53af263f1adb40e4",
    )
}

const FIRST_NODE_NAMESPACE: &str = "sequencer-test-3-node-0";

fn testing_env_3_node_0_config_override() -> ConfigOverride {
    ConfigOverride::new(
        testing_env_3_deployment_config_override(),
        create_hybrid_instance_config_override(0, FIRST_NODE_NAMESPACE),
    )
}
fn testing_env_3_node_1_config_override() -> ConfigOverride {
    ConfigOverride::new(
        testing_env_3_deployment_config_override(),
        create_hybrid_instance_config_override(1, FIRST_NODE_NAMESPACE),
    )
}
fn testing_env_3_node_2_config_override() -> ConfigOverride {
    ConfigOverride::new(
        testing_env_3_deployment_config_override(),
        create_hybrid_instance_config_override(2, FIRST_NODE_NAMESPACE),
    )
}
fn testing_env_3_node_3_config_override() -> ConfigOverride {
    ConfigOverride::new(
        testing_env_3_deployment_config_override(),
        create_hybrid_instance_config_override(3, FIRST_NODE_NAMESPACE),
    )
}

fn get_ingress_params() -> IngressParams {
    IngressParams::new(
        TESTING_ENV_3_INGRESS_DOMAIN.to_string(),
        Some(vec![TESTING_ENV_3_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
    )
}

const TESTING_ENV_3_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME: &str =
    "sn-test-sepolia-3-sepolia.gateway-proxy.sw-dev.io";

const TESTING_ENV_3_INGRESS_DOMAIN: &str = "sw-dev.io";

pub(crate) fn testing_env_3_hybrid_deployment_node_0() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::TestingEnvThree,
        "integration_hybrid_node_0",
        Some(ExternalSecret::new("sequencer-test-3-node-0")),
        PathBuf::from(BASE_APP_CONFIG_PATH),
        testing_env_3_node_0_config_override(),
        get_ingress_params(),
    )
}

pub(crate) fn testing_env_3_hybrid_deployment_node_1() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::TestingEnvThree,
        "integration_hybrid_node_1",
        Some(ExternalSecret::new("sequencer-test-3-node-1")),
        PathBuf::from(BASE_APP_CONFIG_PATH),
        testing_env_3_node_1_config_override(),
        get_ingress_params(),
    )
}

pub(crate) fn testing_env_3_hybrid_deployment_node_2() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::TestingEnvThree,
        "integration_hybrid_node_2",
        Some(ExternalSecret::new("sequencer-test-3-node-2")),
        PathBuf::from(BASE_APP_CONFIG_PATH),
        testing_env_3_node_2_config_override(),
        get_ingress_params(),
    )
}

pub(crate) fn testing_env_3_hybrid_deployment_node_3() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::TestingEnvThree,
        "integration_hybrid_node_3",
        Some(ExternalSecret::new("sequencer-test-3-node-3")),
        PathBuf::from(BASE_APP_CONFIG_PATH),
        testing_env_3_node_3_config_override(),
        get_ingress_params(),
    )
}
