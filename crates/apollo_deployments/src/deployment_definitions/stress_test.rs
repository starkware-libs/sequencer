use apollo_infra_utils::template::Template;

use crate::config_override::{ConfigOverride, DeploymentConfigOverride};
use crate::deployment::{Deployment, P2PCommunicationType};
use crate::deployment_definitions::{Environment, StateSyncType};
use crate::deployments::hybrid::{create_hybrid_instance_config_override, INSTANCE_NAME_FORMAT};
use crate::k8s::{ExternalSecret, IngressParams};
use crate::service::NodeType;

const STRESS_TEST_NODE_IDS: [usize; 3] = [0, 1, 2];
const STRESS_TEST_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME: &str = "apollo-stresstest-dev.sw-dev.io";
const STRESS_TEST_INGRESS_DOMAIN: &str = "sw-dev.io";
const SECRET_NAME_FORMAT: Template = Template("apollo-stresstest-dev-{}");
const NODE_NAMESPACE_FORMAT: Template = Template("apollo-stresstest-dev-{}");

pub(crate) fn stress_test_hybrid_deployments() -> Vec<Deployment> {
    STRESS_TEST_NODE_IDS
        .map(|i| stress_test_hybrid_deployment_node(i, P2PCommunicationType::Internal))
        .to_vec()
}

fn stress_test_deployment_config_override() -> DeploymentConfigOverride {
    DeploymentConfigOverride::new(
        "0x4fA369fEBf0C574ea05EC12bC0e1Bc9Cd461Dd0f",
        "INTERNAL_STRESS_TEST",
        "0x7e813ecf3e7b3e14f07bd2f68cb4a3d12110e3c75ec5a63de3d2dacf1852904",
        "http://feeder-gateway.starknet-0-14-0-stress-test-03:9713/",
        "0x2208cce4221df1f35943958340abc812aa79a8f6a533bff4ee00416d3d06cd6",
        None,
        STRESS_TEST_NODE_IDS.len(),
        StateSyncType::Central,
    )
}

fn stress_test_hybrid_deployment_node(
    id: usize,
    p2p_communication_type: P2PCommunicationType,
) -> Deployment {
    Deployment::new(
        NodeType::Hybrid,
        Environment::StressTest,
        &INSTANCE_NAME_FORMAT.format(&[&id]),
        Some(ExternalSecret::new(SECRET_NAME_FORMAT.format(&[&id]))),
        ConfigOverride::new(
            stress_test_deployment_config_override(),
            create_hybrid_instance_config_override(
                id,
                NODE_NAMESPACE_FORMAT,
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
