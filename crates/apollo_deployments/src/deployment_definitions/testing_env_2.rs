use std::path::PathBuf;

use crate::deployment::{
    create_hybrid_instance_config_override,
    format_node_id,
    ConfigOverride,
    Deployment,
    DeploymentConfigOverride,
    DeploymentType,
    P2PCommunicationType,
    PragmaDomain,
};
use crate::deployment_definitions::{Environment, BASE_APP_CONFIG_PATH};
use crate::service::{DeploymentName, ExternalSecret, IngressParams};

const TESTING_ENV_2_NODE_IDS: [usize; 4] = [0, 1, 2, 3];
const TESTING_ENV_2_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME: &str =
    "sn-test-sepolia-2-sepolia.gateway-proxy.sw-dev.io";
const TESTING_ENV_2_INGRESS_DOMAIN: &str = "sw-dev.io";
const FIRST_NODE_NAMESPACE: &str = "sequencer-test-sepolia-0";
const INSTANCE_NAME_FORMAT: &str = "integration_hybrid_node_{}";
const SECRET_NAME_FORMAT: &str = "sequencer-test-sepolia-{}";

pub(crate) fn testing_env_2_hybrid_deployments() -> Vec<Deployment> {
    TESTING_ENV_2_NODE_IDS
        .map(|i| {
            testing_env_2_hybrid_deployment_node(
                i,
                DeploymentType::Operational,
                P2PCommunicationType::Internal,
            )
        })
        .to_vec()
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
        PragmaDomain::Dev,
    )
}

fn testing_env_2_hybrid_deployment_node(
    id: usize,
    deployment_type: DeploymentType,
    p2p_communication_type: P2PCommunicationType,
) -> Deployment {
    Deployment::new(
        DeploymentName::HybridNode,
        Environment::TestingEnvTwo,
        &format_node_id(INSTANCE_NAME_FORMAT, id),
        Some(ExternalSecret::new(format_node_id(SECRET_NAME_FORMAT, id))),
        PathBuf::from(BASE_APP_CONFIG_PATH),
        ConfigOverride::new(
            testing_env_2_deployment_config_override(),
            create_hybrid_instance_config_override(
                id,
                FIRST_NODE_NAMESPACE,
                deployment_type,
                p2p_communication_type,
                TESTING_ENV_2_INGRESS_DOMAIN,
            ),
        ),
        IngressParams::new(
            TESTING_ENV_2_INGRESS_DOMAIN.to_string(),
            Some(vec![TESTING_ENV_2_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
        ),
    )
}
