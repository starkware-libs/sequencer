use std::path::PathBuf;

use starknet_api::core::ChainId;

use crate::deployment::{
    ConfigOverride,
    Deployment,
    DeploymentConfigOverride,
    InstanceConfigOverride,
};
use crate::deployment_definitions::{Environment, BASE_APP_CONFIG_PATH};
use crate::service::{DeploymentName, ExternalSecret};

const TESTING_ENV_3_DEPLOYMENT_CONFIG_OVERRIDE: DeploymentConfigOverride =
    DeploymentConfigOverride::new(
        "0xa23a6BA7DA61988D2420dAE9F10eE964552459d5",
        "SN_GOERLI",
        "0x7c07a3eec8ff611328722c3fc3e5d2e4ef2f60740c0bf86c756606036b74c16",
        "https://fgw-sn-test-sepolia-3-sepolia.gateway-proxy.sw-dev.io",
        "0x54a93d918d62b2fb62b25e77d9cb693bd277ab7e6fa236e53af263f1adb40e4",
    );

fn testing_env_3_node_0_instance_config_override() -> InstanceConfigOverride {
    InstanceConfigOverride::new(
        "",
        true,
        "0x0101010101010101010101010101010101010101010101010101010101010101",
        "",
        true,
        "0x0101010101010101010101010101010101010101010101010101010101010101",
        "0x1",
    )
}

fn testing_env_3_node_1_instance_config_override() -> InstanceConfigOverride {
    InstanceConfigOverride::new(
        "/dns/sequencer-core-service.sequencer-test-3-node-0.svc.cluster.local/tcp/53080/p2p/\
         12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        false,
        "0x0101010101010101010101010101010101010101010101010101010101010102",
        "/dns/sequencer-mempool-service.sequencer-test-3-node-0.svc.cluster.local/tcp/53200/p2p/\
         12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        false,
        "0x0101010101010101010101010101010101010101010101010101010101010102",
        "0x2",
    )
}

fn testing_env_3_node_2_instance_config_override() -> InstanceConfigOverride {
    InstanceConfigOverride::new(
        "/dns/sequencer-core-service.sequencer-test-3-node-0.svc.cluster.local/tcp/53080/p2p/\
         12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        false,
        "0x0101010101010101010101010101010101010101010101010101010101010103",
        "/dns/sequencer-mempool-service.sequencer-test-3-node-0.svc.cluster.local/tcp/53200/p2p/\
         12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        false,
        "0x0101010101010101010101010101010101010101010101010101010101010103",
        "0x3",
    )
}

fn testing_env_3_node_3_instance_config_override() -> InstanceConfigOverride {
    InstanceConfigOverride::new(
        "/dns/sequencer-core-service.sequencer-test-3-node-0.svc.cluster.local/tcp/53080/p2p/\
         12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        false,
        "0x0101010101010101010101010101010101010101010101010101010101010104",
        "/dns/sequencer-mempool-service.sequencer-test-3-node-0.svc.cluster.local/tcp/53200/p2p/\
         12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        false,
        "0x0101010101010101010101010101010101010101010101010101010101010104",
        "0x4",
    )
}

fn testing_env_3_node_0_config_override() -> ConfigOverride {
    ConfigOverride::new(
        TESTING_ENV_3_DEPLOYMENT_CONFIG_OVERRIDE,
        testing_env_3_node_0_instance_config_override(),
    )
}
fn testing_env_3_node_1_config_override() -> ConfigOverride {
    ConfigOverride::new(
        TESTING_ENV_3_DEPLOYMENT_CONFIG_OVERRIDE,
        testing_env_3_node_1_instance_config_override(),
    )
}
fn testing_env_3_node_2_config_override() -> ConfigOverride {
    ConfigOverride::new(
        TESTING_ENV_3_DEPLOYMENT_CONFIG_OVERRIDE,
        testing_env_3_node_2_instance_config_override(),
    )
}
fn testing_env_3_node_3_config_override() -> ConfigOverride {
    ConfigOverride::new(
        TESTING_ENV_3_DEPLOYMENT_CONFIG_OVERRIDE,
        testing_env_3_node_3_instance_config_override(),
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
        TESTING_ENV_3_INGRESS_DOMAIN.to_string(),
        Some(vec![TESTING_ENV_3_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
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
        TESTING_ENV_3_INGRESS_DOMAIN.to_string(),
        Some(vec![TESTING_ENV_3_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
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
        TESTING_ENV_3_INGRESS_DOMAIN.to_string(),
        Some(vec![TESTING_ENV_3_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
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
        TESTING_ENV_3_INGRESS_DOMAIN.to_string(),
        Some(vec![TESTING_ENV_3_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
    )
}
