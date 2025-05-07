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

const SEPOLIA_INTEGRATION_DEPLOYMENT_CONFIG_OVERRIDE: DeploymentConfigOverride =
    DeploymentConfigOverride::new(
        "0x4737c0c1B4D5b1A687B42610DdabEE781152359c",
        "SN_INTEGRATION_SEPOLIA",
        "0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7",
        "https://feeder.integration-sepolia.starknet.io/",
        "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d",
    );

const SEPOLIA_INTEGRATION_NODE_0_INSTANCE_CONFIG_OVERRIDE: InstanceConfigOverride =
    InstanceConfigOverride::new(
        "",
        true,
        "0x0101010101010101010101010101010101010101010101010101010101010101",
        "",
        true,
        "0x0101010101010101010101010101010101010101010101010101010101010101",
        "0x1",
    );

const SEPOLIA_INTEGRATION_NODE_1_INSTANCE_CONFIG_OVERRIDE: InstanceConfigOverride =
    InstanceConfigOverride::new(
        "/dns/sequencer-core-service.apollo-sepolia-integration-0.svc.cluster.local/tcp/53080/p2p/\
         12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        false,
        "0x0101010101010101010101010101010101010101010101010101010101010102",
        "/dns/sequencer-mempool-service.apollo-sepolia-integration-0.svc.cluster.local/tcp/53200/\
         p2p/12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        false,
        "0x0101010101010101010101010101010101010101010101010101010101010102",
        "0x2",
    );

const SEPOLIA_INTEGRATION_NODE_2_INSTANCE_CONFIG_OVERRIDE: InstanceConfigOverride =
    InstanceConfigOverride::new(
        "/dns/sequencer-core-service.apollo-sepolia-integration-0.svc.cluster.local/tcp/53080/p2p/\
         12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        false,
        "0x0101010101010101010101010101010101010101010101010101010101010103",
        "/dns/sequencer-mempool-service.apollo-sepolia-integration-0.svc.cluster.local/tcp/53200/\
         p2p/12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        false,
        "0x0101010101010101010101010101010101010101010101010101010101010103",
        "0x3",
    );

const SEPOLIA_INTEGRATION_NODE_3_INSTANCE_CONFIG_OVERRIDE: InstanceConfigOverride =
    InstanceConfigOverride::new(
        "/dns/sequencer-core-service.apollo-sepolia-integration-0.svc.cluster.local/tcp/53080/p2p/\
         12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        false,
        "0x0101010101010101010101010101010101010101010101010101010101010104",
        "/dns/sequencer-mempool-service.apollo-sepolia-integration-0.svc.cluster.local/tcp/53200/\
         p2p/12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        false,
        "0x0101010101010101010101010101010101010101010101010101010101010104",
        "0x4",
    );

fn sepolia_integration_node_0_config_override() -> ConfigOverride {
    ConfigOverride::new(
        SEPOLIA_INTEGRATION_DEPLOYMENT_CONFIG_OVERRIDE,
        SEPOLIA_INTEGRATION_NODE_0_INSTANCE_CONFIG_OVERRIDE,
    )
}
fn sepolia_integration_node_1_config_override() -> ConfigOverride {
    ConfigOverride::new(
        SEPOLIA_INTEGRATION_DEPLOYMENT_CONFIG_OVERRIDE,
        SEPOLIA_INTEGRATION_NODE_1_INSTANCE_CONFIG_OVERRIDE,
    )
}
fn sepolia_integration_node_2_config_override() -> ConfigOverride {
    ConfigOverride::new(
        SEPOLIA_INTEGRATION_DEPLOYMENT_CONFIG_OVERRIDE,
        SEPOLIA_INTEGRATION_NODE_2_INSTANCE_CONFIG_OVERRIDE,
    )
}
fn sepolia_integration_node_3_config_override() -> ConfigOverride {
    ConfigOverride::new(
        SEPOLIA_INTEGRATION_DEPLOYMENT_CONFIG_OVERRIDE,
        SEPOLIA_INTEGRATION_NODE_3_INSTANCE_CONFIG_OVERRIDE,
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
        SEPOLIA_INTEGRATION_INGRESS_DOMAIN.to_string(),
        Some(vec![SEPOLIA_INTEGRATION_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
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
        SEPOLIA_INTEGRATION_INGRESS_DOMAIN.to_string(),
        Some(vec![SEPOLIA_INTEGRATION_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
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
        SEPOLIA_INTEGRATION_INGRESS_DOMAIN.to_string(),
        Some(vec![SEPOLIA_INTEGRATION_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
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
        SEPOLIA_INTEGRATION_INGRESS_DOMAIN.to_string(),
        Some(vec![SEPOLIA_INTEGRATION_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
    )
}
