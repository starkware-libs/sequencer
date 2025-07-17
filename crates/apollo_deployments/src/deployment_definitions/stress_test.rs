use apollo_infra_utils::template::Template;
use starknet_api::block::BlockNumber;
use url::Url;

use crate::config_override::{ConfigOverride, DeploymentConfigOverride};
use crate::deployment::{Deployment, P2PCommunicationType};
use crate::deployment_definitions::{CloudK8sEnvironment, Environment, StateSyncType};
use crate::deployments::hybrid::{create_hybrid_instance_config_override, INSTANCE_NAME_FORMAT};
use crate::k8s::{ExternalSecret, IngressParams};
use crate::service::NodeType;

const NODE_IDS: [usize; 3] = [0, 1, 2];
const STRESS_TEST_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME: &str = "apollo-stresstest-dev.sw-dev.io";
const STRESS_TEST_INGRESS_DOMAIN: &str = "sw-dev.io";
const SECRET_NAME_FORMAT: Template = Template("apollo-stresstest-dev-{}");
const NODE_NAMESPACE_FORMAT: Template = Template("apollo-stresstest-dev-{}");

const STARKNET_CONTRACT_ADDRESS: &str = "0x4fA369fEBf0C574ea05EC12bC0e1Bc9Cd461Dd0f";
const CHAIN_ID: &str = "INTERNAL_STRESS_TEST";
const ETH_FEE_TOKEN_ADDRESS: &str =
    "0x7e813ecf3e7b3e14f07bd2f68cb4a3d12110e3c75ec5a63de3d2dacf1852904";
const STARKNET_GATEWAY_URL: &str = "http://feeder-gateway.starknet-0-14-0-stress-test-03:9713/";
const STRK_FEE_TOKEN_ADDRESS: &str =
    "0x2208cce4221df1f35943958340abc812aa79a8f6a533bff4ee00416d3d06cd6";
const L1_STARTUP_HEIGHT_OVERRIDE: Option<BlockNumber> = None;
const STATE_SYNC_TYPE: StateSyncType = StateSyncType::Central;

pub(crate) fn stress_test_hybrid_deployments() -> Vec<Deployment> {
    NODE_IDS.map(|i| stress_test_hybrid_deployment_node(i, P2PCommunicationType::Internal)).to_vec()
}

fn deployment_config_override() -> DeploymentConfigOverride {
    DeploymentConfigOverride::new(
        STARKNET_CONTRACT_ADDRESS,
        CHAIN_ID,
        ETH_FEE_TOKEN_ADDRESS,
        Url::parse(STARKNET_GATEWAY_URL).expect("Invalid URL"),
        STRK_FEE_TOKEN_ADDRESS,
        L1_STARTUP_HEIGHT_OVERRIDE,
        NODE_IDS.len(),
        STATE_SYNC_TYPE,
    )
}

fn stress_test_hybrid_deployment_node(
    id: usize,
    p2p_communication_type: P2PCommunicationType,
) -> Deployment {
    Deployment::new(
        NodeType::Hybrid,
        Environment::CloudK8s(CloudK8sEnvironment::StressTest),
        &INSTANCE_NAME_FORMAT.format(&[&id]),
        Some(ExternalSecret::new(SECRET_NAME_FORMAT.format(&[&id]))),
        ConfigOverride::new(
            deployment_config_override(),
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
