use apollo_infra_utils::template::Template;
use starknet_api::block::BlockNumber;
use url::Url;

use crate::config_override::{ConfigOverride, DeploymentConfigOverride};
use crate::deployment::{Deployment, P2PCommunicationType};
use crate::deployment_definitions::{CloudK8sEnvironment, Environment, StateSyncType};
use crate::deployments::hybrid::{create_hybrid_instance_config_override, INSTANCE_NAME_FORMAT};
use crate::k8s::{ExternalSecret, IngressParams, K8sServiceConfigParams};
use crate::service::NodeType;

const NODE_IDS: [usize; 3] = [0, 1, 2];
const UPGRADE_TEST_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME: &str =
    "sn-alpha-test-upgrade.gateway-proxy.sw-dev.io";
const UPGRADE_TEST_INGRESS_DOMAIN: &str = "sw-dev.io";
const SECRET_NAME_FORMAT: Template = Template("apollo-alpha-test-{}");
const NODE_NAMESPACE_FORMAT: Template = Template("apollo-alpha-test-{}");

const STARKNET_CONTRACT_ADDRESS: &str = "0x9b8A6361d204a0C1F93d5194763538057444d958";
const CHAIN_ID: &str = "SN_GOERLI";
const ETH_FEE_TOKEN_ADDRESS: &str =
    "0x7c07a3eec8ff611328722c3fc3e5d2e4ef2f60740c0bf86c756606036b74c16";
const STARKNET_GATEWAY_URL: &str = "https://feeder.sn-alpha-test-upgrade.gateway-proxy.sw-dev.io";
const STRK_FEE_TOKEN_ADDRESS: &str =
    "0x54a93d918d62b2fb62b25e77d9cb693bd277ab7e6fa236e53af263f1adb40e4";
const L1_STARTUP_HEIGHT_OVERRIDE: Option<BlockNumber> = None;
const STATE_SYNC_TYPE: StateSyncType = StateSyncType::Central;

pub(crate) fn upgrade_test_hybrid_deployments() -> Vec<Deployment> {
    NODE_IDS
        .map(|i| upgrade_test_hybrid_deployment_node(i, P2PCommunicationType::External))
        .to_vec()
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

fn upgrade_test_hybrid_deployment_node(
    id: usize,
    p2p_communication_type: P2PCommunicationType,
) -> Deployment {
    Deployment::new(
        NodeType::Hybrid,
        Environment::CloudK8s(CloudK8sEnvironment::UpgradeTest),
        &INSTANCE_NAME_FORMAT.format(&[&id]),
        Some(ExternalSecret::new(SECRET_NAME_FORMAT.format(&[&id]))),
        ConfigOverride::new(
            deployment_config_override(),
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
