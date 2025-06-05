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

const TESTING_ENV_2_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME: &str =
    "sn-test-sepolia-2-sepolia.gateway-proxy.sw-dev.io";
const TESTING_ENV_2_INGRESS_DOMAIN: &str = "sw-dev.io";
const FIRST_NODE_NAMESPACE: &str = "sequencer-test-sepolia-0";
const INSTANCE_NAME_FORMAT: &str = "integration_hybrid_node_{}";
const SECRET_NAME_FORMAT: &str = "sequencer-test-sepolia-{}";

pub(crate) fn testing_env_2_hybrid_deployments() -> Vec<Deployment> {
    vec![
        testing_env_2_hybrid_deployment_node(0, DeploymentType::Operational),
        testing_env_2_hybrid_deployment_node(1, DeploymentType::Operational),
        testing_env_2_hybrid_deployment_node(2, DeploymentType::Operational),
        testing_env_2_hybrid_deployment_node(3, DeploymentType::Operational),
    ]
}

// TODO(Tsabary): for all envs, define the values as constants at the top of the module, and cancel
// the inner function calls.
fn testing_env_2_deployment_config_override() -> DeploymentConfigOverride {
    DeploymentConfigOverride::new(
        "0xA43812F9C610851daF67c5FA36606Ea8c8Fa7caE",
        "SN_GOERLI",
        "0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7",
        "https://fgw-sn-test-sepolia-2-sepolia.gateway-proxy.sw-dev.io",
        "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d",
    )
}

fn testing_env_2_hybrid_deployment_node(id: usize, deployment_type: DeploymentType) -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::TestingEnvTwo,
        &format_node_id(INSTANCE_NAME_FORMAT, id),
        Some(ExternalSecret::new(format_node_id(SECRET_NAME_FORMAT, id))),
        PathBuf::from(BASE_APP_CONFIG_PATH),
        ConfigOverride::new(
            testing_env_2_deployment_config_override(),
            create_hybrid_instance_config_override(id, FIRST_NODE_NAMESPACE, deployment_type),
        ),
        IngressParams::new(
            TESTING_ENV_2_INGRESS_DOMAIN.to_string(),
            Some(vec![TESTING_ENV_2_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
        ),
    )
}
