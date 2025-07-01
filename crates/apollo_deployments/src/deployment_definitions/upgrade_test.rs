use apollo_infra_utils::template::Template;

use crate::config_override::{ConfigOverride, DeploymentConfigOverride};
use crate::deployment::{Deployment, P2PCommunicationType, PragmaDomain};
use crate::deployment_definitions::Environment;
use crate::deployments::hybrid::{create_hybrid_instance_config_override, INSTANCE_NAME_FORMAT};
use crate::k8s::{ExternalSecret, IngressParams, K8sServiceConfigParams};
use crate::service::NodeType;

const UPGRADE_TEST_NODE_IDS: [usize; 3] = [0, 1, 2];
const UPGRADE_TEST_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME: &str =
    "sn-alpha-test-upgrade.gateway-proxy.sw-dev.io";
const UPGRADE_TEST_INGRESS_DOMAIN: &str = "sw-dev.io";
const SECRET_NAME_FORMAT: Template = Template("apollo-alpha-test-{}");
const NODE_NAMESPACE_FORMAT: Template = Template("apollo-alpha-test-{}");

pub(crate) fn upgrade_test_hybrid_deployments() -> Vec<Deployment> {
    UPGRADE_TEST_NODE_IDS
        .map(|i| upgrade_test_hybrid_deployment_node(i, P2PCommunicationType::External))
        .to_vec()
}

// TODO(Tsabary): for all envs, define the values as constants at the top of the module, and cancel
// the inner function calls.
fn upgrade_test_deployment_config_override() -> DeploymentConfigOverride {
    DeploymentConfigOverride::new(
        "0x9b8A6361d204a0C1F93d5194763538057444d958",
        "SN_GOERLI",
        "0x7c07a3eec8ff611328722c3fc3e5d2e4ef2f60740c0bf86c756606036b74c16",
        "feeder.sn-alpha-test-upgrade.gateway-proxy.sw-dev.io",
        "0x54a93d918d62b2fb62b25e77d9cb693bd277ab7e6fa236e53af263f1adb40e4",
        PragmaDomain::Dev,
        None,
        UPGRADE_TEST_NODE_IDS.len(),
    )
}

fn upgrade_test_hybrid_deployment_node(
    id: usize,
    p2p_communication_type: P2PCommunicationType,
) -> Deployment {
    Deployment::new(
        NodeType::Hybrid,
        Environment::UpgradeTest,
        &INSTANCE_NAME_FORMAT.format(&[&id]),
        Some(ExternalSecret::new(SECRET_NAME_FORMAT.format(&[&id]))),
        ConfigOverride::new(
            upgrade_test_deployment_config_override(),
            create_hybrid_instance_config_override(
                id,
                NODE_NAMESPACE_FORMAT,
                p2p_communication_type,
                UPGRADE_TEST_INGRESS_DOMAIN,
            ),
        ),
        IngressParams::new(
            UPGRADE_TEST_INGRESS_DOMAIN.to_string(),
            Some(vec![UPGRADE_TEST_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
        ),
        Some(K8sServiceConfigParams::new(
            NODE_NAMESPACE_FORMAT.format(&[&id]),
            UPGRADE_TEST_INGRESS_DOMAIN.to_string(),
            P2PCommunicationType::External,
        )),
    )
}
