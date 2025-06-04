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

const TESTING_ENV_3_NODE_IDS: [usize; 4] = [0, 1, 2, 3];
const TESTING_ENV_3_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME: &str =
    "sn-test-sepolia-3-sepolia.gateway-proxy.sw-dev.io";
const TESTING_ENV_3_INGRESS_DOMAIN: &str = "sw-dev.io";
const FIRST_NODE_NAMESPACE: &str = "sequencer-test-3-node-0";
const INSTANCE_NAME_FORMAT: &str = "integration_hybrid_node_{}";
const SECRET_NAME_FORMAT: &str = "sequencer-test-3-node-{}";

pub(crate) fn testing_env_3_hybrid_deployments() -> Vec<Deployment> {
    TESTING_ENV_3_NODE_IDS
        .map(|i| testing_env_3_hybrid_deployment_node(i, DeploymentType::Operational))
        .to_vec()
}

fn testing_env_3_deployment_config_override() -> DeploymentConfigOverride {
    DeploymentConfigOverride::new(
        "0xa23a6BA7DA61988D2420dAE9F10eE964552459d5",
        "SN_GOERLI",
        "0x7c07a3eec8ff611328722c3fc3e5d2e4ef2f60740c0bf86c756606036b74c16",
        "https://fgw-sn-test-sepolia-3-sepolia.gateway-proxy.sw-dev.io",
        "0x54a93d918d62b2fb62b25e77d9cb693bd277ab7e6fa236e53af263f1adb40e4",
    )
}

fn testing_env_3_hybrid_deployment_node(id: usize, deployment_type: DeploymentType) -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::TestingEnvThree,
        &format_node_id(INSTANCE_NAME_FORMAT, id),
        Some(ExternalSecret::new(format_node_id(SECRET_NAME_FORMAT, id))),
        PathBuf::from(BASE_APP_CONFIG_PATH),
        ConfigOverride::new(
            testing_env_3_deployment_config_override(),
            create_hybrid_instance_config_override(id, FIRST_NODE_NAMESPACE, deployment_type),
        ),
        IngressParams::new(
            TESTING_ENV_3_INGRESS_DOMAIN.to_string(),
            Some(vec![TESTING_ENV_3_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
        ),
    )
}
