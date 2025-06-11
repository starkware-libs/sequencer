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

const CLUSTER_TESTING_NODE_IDS: [usize; 4] = [0, 1, 2, 3];
const CLUSTER_TESTING_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME: &str =
    "sn-test-sepolia-2-sepolia.gateway-proxy.sw-dev.io";
const CLUSTER_TESTING_INGRESS_DOMAIN: &str = "sw-dev.io";
const FIRST_NODE_NAMESPACE: &str = "apollo-preconfirmation-test-0";
const INSTANCE_NAME_FORMAT: &str = "apollo-preconfirmation-test-{}";
const SECRET_NAME_FORMAT: &str = "apollo-preconfirmation-test-{}";

pub(crate) fn cluster_testing_hybrid_deployments() -> Vec<Deployment> {
    CLUSTER_TESTING_NODE_IDS
        .map(|i| {
            cluster_testing_hybrid_deployment_node(
                i,
                DeploymentType::Operational,
                P2PCommunicationType::Internal,
            )
        })
        .to_vec()
}

// TODO(Tsabary): for all envs, define the values as constants at the top of the module, and cancel
// the inner function calls.
fn cluster_testing_deployment_config_override() -> DeploymentConfigOverride {
    DeploymentConfigOverride::new(
        "0x4fA369fEBf0C574ea05EC12bC0e1Bc9Cd461Dd0f",
        "SN_GOERLI",
        "0x497d1c054cec40f64454b45deecdc83e0c7f7b961c63531eae03748abd95350",
        "http://feeder-gateway.starknet-0-14-0-stress-test:9713",
        "0x4fa9355c504fa2de263bd7920644b5e48794fe1450ec2a6526518ad77d6a567",
        PragmaDomain::Dev,
    )
}

fn cluster_testing_hybrid_deployment_node(
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
            cluster_testing_deployment_config_override(),
            create_hybrid_instance_config_override(
                id,
                FIRST_NODE_NAMESPACE,
                deployment_type,
                p2p_communication_type,
                CLUSTER_TESTING_INGRESS_DOMAIN,
            ),
        ),
        IngressParams::new(
            CLUSTER_TESTING_INGRESS_DOMAIN.to_string(),
            Some(vec![CLUSTER_TESTING_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
        ),
    )
}
