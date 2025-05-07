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

const TESTING_ENV_2_DEPLOYMENT_CONFIG_OVERRIDE: DeploymentConfigOverride =
    DeploymentConfigOverride::new(
        "0xA43812F9C610851daF67c5FA36606Ea8c8Fa7caE",
        "SN_GOERLI",
        "0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7",
        "https://fgw-sn-test-sepolia-2-sepolia.gateway-proxy.sw-dev.io",
        "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d",
    );

const TESTING_ENV_2_NODE_0_INSTANCE_CONFIG_OVERRIDE: InstanceConfigOverride =
    InstanceConfigOverride::new(
        "",
        true,
        "0x0101010101010101010101010101010101010101010101010101010101010101",
        "",
        true,
        "0x0101010101010101010101010101010101010101010101010101010101010101",
        "0x1",
    );

const TESTING_ENV_2_NODE_1_INSTANCE_CONFIG_OVERRIDE: InstanceConfigOverride =
    InstanceConfigOverride::new(
        "/dns/sequencer-core-service.sequencer-test-sepolia-0.svc.cluster.local/tcp/53080/p2p/\
         12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        false,
        "0x0101010101010101010101010101010101010101010101010101010101010102",
        "/dns/sequencer-mempool-service.sequencer-test-sepolia-0.svc.cluster.local/tcp/53200/p2p/\
         12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        false,
        "0x0101010101010101010101010101010101010101010101010101010101010102",
        "0x2",
    );

const TESTING_ENV_2_NODE_2_INSTANCE_CONFIG_OVERRIDE: InstanceConfigOverride =
    InstanceConfigOverride::new(
        "/dns/sequencer-core-service.sequencer-test-sepolia-0.svc.cluster.local/tcp/53080/p2p/\
         12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        false,
        "0x0101010101010101010101010101010101010101010101010101010101010103",
        "/dns/sequencer-mempool-service.sequencer-test-sepolia-0.svc.cluster.local/tcp/53200/p2p/\
         12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        false,
        "0x0101010101010101010101010101010101010101010101010101010101010103",
        "0x3",
    );

const TESTING_ENV_2_NODE_3_INSTANCE_CONFIG_OVERRIDE: InstanceConfigOverride =
    InstanceConfigOverride::new(
        "/dns/sequencer-core-service.sequencer-test-sepolia-0.svc.cluster.local/tcp/53080/p2p/\
         12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        false,
        "0x0101010101010101010101010101010101010101010101010101010101010104",
        "/dns/sequencer-mempool-service.sequencer-test-sepolia-0.svc.cluster.local/tcp/53200/p2p/\
         12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        false,
        "0x0101010101010101010101010101010101010101010101010101010101010104",
        "0x4",
    );

fn testing_env_2_node_0_config_override() -> ConfigOverride {
    ConfigOverride::new(
        TESTING_ENV_2_DEPLOYMENT_CONFIG_OVERRIDE,
        TESTING_ENV_2_NODE_0_INSTANCE_CONFIG_OVERRIDE,
    )
}
fn testing_env_2_node_1_config_override() -> ConfigOverride {
    ConfigOverride::new(
        TESTING_ENV_2_DEPLOYMENT_CONFIG_OVERRIDE,
        TESTING_ENV_2_NODE_1_INSTANCE_CONFIG_OVERRIDE,
    )
}
fn testing_env_2_node_2_config_override() -> ConfigOverride {
    ConfigOverride::new(
        TESTING_ENV_2_DEPLOYMENT_CONFIG_OVERRIDE,
        TESTING_ENV_2_NODE_2_INSTANCE_CONFIG_OVERRIDE,
    )
}
fn testing_env_2_node_3_config_override() -> ConfigOverride {
    ConfigOverride::new(
        TESTING_ENV_2_DEPLOYMENT_CONFIG_OVERRIDE,
        TESTING_ENV_2_NODE_3_INSTANCE_CONFIG_OVERRIDE,
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
