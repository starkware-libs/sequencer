use std::path::PathBuf;

use starknet_api::core::ChainId;

use crate::deployment::{
    create_hybrid_instance_config_override,
    format_node_id,
    ConfigOverride,
    Deployment,
    DeploymentConfigOverride,
    DeploymentType,
};
use crate::deployment_definitions::{Environment, BASE_APP_CONFIG_PATH};
use crate::service::{DeploymentName, ExternalSecret, IngressParams};

const SEPOLIA_INTEGRATION_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME: &str =
    "integration-sepolia.starknet.io";
const SEPOLIA_INTEGRATION_INGRESS_DOMAIN: &str = "starknet.io";
const FIRST_NODE_NAMESPACE: &str = "apollo-sepolia-integration-0";
const INSTANCE_NAME_FORMAT: &str = "integration_hybrid_node_{}";
const SECRET_NAME_FORMAT: &str = "apollo-sepolia-integration-{}";

pub(crate) fn sepolia_integration_hybrid_deployments() -> Vec<Deployment> {
    vec![
        sepolia_integration_hybrid_deployment_node(0, DeploymentType::Operational),
        sepolia_integration_hybrid_deployment_node(1, DeploymentType::Operational),
        sepolia_integration_hybrid_deployment_node(2, DeploymentType::Operational),
        sepolia_integration_hybrid_deployment_node(3, DeploymentType::Operational),
    ]
}

fn sepolia_integration_deployment_config_override() -> DeploymentConfigOverride {
    DeploymentConfigOverride::new(
        "0x4737c0c1B4D5b1A687B42610DdabEE781152359c",
        "SN_INTEGRATION_SEPOLIA",
        "0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7",
        "https://feeder.integration-sepolia.starknet.io/",
        "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d",
    )
}

fn sepolia_integration_hybrid_deployment_node(
    id: usize,
    deployment_type: DeploymentType,
) -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::SepoliaIntegration,
        &format_node_id(INSTANCE_NAME_FORMAT, id),
        Some(ExternalSecret::new(format_node_id(SECRET_NAME_FORMAT, id))),
        PathBuf::from(BASE_APP_CONFIG_PATH),
        ConfigOverride::new(
            sepolia_integration_deployment_config_override(),
            create_hybrid_instance_config_override(id, FIRST_NODE_NAMESPACE, deployment_type),
        ),
        IngressParams::new(
            SEPOLIA_INTEGRATION_INGRESS_DOMAIN.to_string(),
            Some(vec![SEPOLIA_INTEGRATION_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
        ),
    )
}
