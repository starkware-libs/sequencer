use std::path::PathBuf;

use apollo_infra_utils::template::Template;

use crate::config_override::{ConfigOverride, DeploymentConfigOverride};
use crate::deployment::{Deployment, P2PCommunicationType, PragmaDomain};
use crate::deployment_definitions::{Environment, BASE_APP_CONFIG_PATH};
use crate::deployments::hybrid::create_hybrid_instance_config_override;
use crate::k8s::{ExternalSecret, IngressParams, K8sServiceConfigParams};
use crate::service::DeploymentName;

// TODO(Tsabary): note this env has configs for 4 despite needing only 3. Delete when we're done
// with it.
const TESTING_ENV_3_NODE_IDS: [(usize, P2PCommunicationType); 4] = [
    (0, P2PCommunicationType::Internal),
    (1, P2PCommunicationType::Internal),
    (2, P2PCommunicationType::Internal),
    (3, P2PCommunicationType::External),
];
const TESTING_ENV_3_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME: &str =
    "sn-test-sepolia-3-sepolia.gateway-proxy.sw-dev.io";
const TESTING_ENV_3_INGRESS_DOMAIN: &str = "sw-dev.io";
const INSTANCE_NAME_FORMAT: Template = Template("integration_hybrid_node_{}");
const SECRET_NAME_FORMAT: Template = Template("sequencer-test-3-node-{}");
const NODE_NAMESPACE_FORMAT: Template = Template("sequencer-test-3-node-{}");

pub(crate) fn testing_env_3_hybrid_deployments() -> Vec<Deployment> {
    TESTING_ENV_3_NODE_IDS
        .map(|(i, p2p_communication_type)| {
            testing_env_3_hybrid_deployment_node(i, p2p_communication_type)
        })
        .to_vec()
}

fn testing_env_3_deployment_config_override() -> DeploymentConfigOverride {
    DeploymentConfigOverride::new(
        "0xa23a6BA7DA61988D2420dAE9F10eE964552459d5",
        "SN_GOERLI",
        "0x7c07a3eec8ff611328722c3fc3e5d2e4ef2f60740c0bf86c756606036b74c16",
        "https://fgw-sn-test-sepolia-3-sepolia.gateway-proxy.sw-dev.io",
        "0x54a93d918d62b2fb62b25e77d9cb693bd277ab7e6fa236e53af263f1adb40e4",
        PragmaDomain::Dev,
        None,
    )
}

// TODO(Tsabary): the domain `TESTING_ENV_3_INGRESS_DOMAIN` is passed multiple times, unify these.
fn testing_env_3_hybrid_deployment_node(
    id: usize,
    p2p_communication_type: P2PCommunicationType,
) -> Deployment {
    Deployment::new(
        DeploymentName::HybridNode,
        Environment::TestingEnvThree,
        &INSTANCE_NAME_FORMAT.format(&[&id]),
        Some(ExternalSecret::new(SECRET_NAME_FORMAT.format(&[&id]))),
        PathBuf::from(BASE_APP_CONFIG_PATH),
        ConfigOverride::new(
            testing_env_3_deployment_config_override(),
            create_hybrid_instance_config_override(
                id,
                NODE_NAMESPACE_FORMAT,
                p2p_communication_type.clone(),
                TESTING_ENV_3_INGRESS_DOMAIN,
            ),
        ),
        IngressParams::new(
            TESTING_ENV_3_INGRESS_DOMAIN.to_string(),
            Some(vec![TESTING_ENV_3_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
        ),
        Some(K8sServiceConfigParams::new(
            NODE_NAMESPACE_FORMAT.format(&[&id]),
            TESTING_ENV_3_INGRESS_DOMAIN.to_string(),
            p2p_communication_type,
        )),
    )
}
