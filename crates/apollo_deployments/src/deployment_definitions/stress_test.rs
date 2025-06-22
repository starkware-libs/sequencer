use std::path::PathBuf;

use crate::config_override::{ConfigOverride, DeploymentConfigOverride};
use crate::deployment::{Deployment, DeploymentType, P2PCommunicationType, PragmaDomain};
use crate::deployment_definitions::{Environment, BASE_APP_CONFIG_PATH};
use crate::deployments::hybrid::create_hybrid_instance_config_override;
use crate::k8s::{ExternalSecret, IngressParams};
use crate::service::DeploymentName;
use crate::utils::format_node_id;

const STRESS_TEST_NODE_IDS: [usize; 3] = [0, 1, 2];
const STRESS_TEST_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME: &str = "apollo-stresstest-dev.sw-dev.io";
const STRESS_TEST_INGRESS_DOMAIN: &str = "sw-dev.io";
const FIRST_NODE_NAMESPACE: &str = "apollo-stresstest-dev-0";
const INSTANCE_NAME_FORMAT: &str = "integration_hybrid_node_{}";
const SECRET_NAME_FORMAT: &str = "apollo-stresstest-dev-{}";

pub(crate) fn stress_test_hybrid_deployments() -> Vec<Deployment> {
    STRESS_TEST_NODE_IDS
        .map(|i| {
            stress_test_hybrid_deployment_node(
                i,
                DeploymentType::Bootstrap,
                P2PCommunicationType::Internal,
            )
        })
        .to_vec()
}

fn stress_test_deployment_config_override() -> DeploymentConfigOverride {
    DeploymentConfigOverride::new(
        "0x4fA369fEBf0C574ea05EC12bC0e1Bc9Cd461Dd0f",
        "SN_GOERLI",
        "0x497d1c054cec40f64454b45deecdc83e0c7f7b961c63531eae03748abd95350",
        "http://feeder-gateway.starknet-0-14-0-stress-test:9713/",
        "0x4fa9355c504fa2de263bd7920644b5e48794fe1450ec2a6526518ad77d6a567",
        PragmaDomain::Dev,
        None,
    )
}

fn stress_test_hybrid_deployment_node(
    id: usize,
    deployment_type: DeploymentType,
    p2p_communication_type: P2PCommunicationType,
) -> Deployment {
    Deployment::new(
        DeploymentName::HybridNode,
        Environment::StressTest,
        &format_node_id(INSTANCE_NAME_FORMAT, id),
        Some(ExternalSecret::new(format_node_id(SECRET_NAME_FORMAT, id))),
        PathBuf::from(BASE_APP_CONFIG_PATH),
        ConfigOverride::new(
            stress_test_deployment_config_override(),
            create_hybrid_instance_config_override(
                id,
                FIRST_NODE_NAMESPACE,
                deployment_type,
                p2p_communication_type,
                STRESS_TEST_INGRESS_DOMAIN,
            ),
        ),
        IngressParams::new(
            STRESS_TEST_INGRESS_DOMAIN.to_string(),
            Some(vec![STRESS_TEST_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
        ),
        None,
    )
}
