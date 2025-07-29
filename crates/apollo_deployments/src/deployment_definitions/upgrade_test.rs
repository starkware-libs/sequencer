use apollo_infra_utils::template::Template;
use starknet_api::block::BlockNumber;
use url::Url;

use crate::config_override::DeploymentConfigOverride;
use crate::deployment::{Deployment, P2PCommunicationType};
use crate::deployment_definitions::{CloudK8sEnvironment, Environment, StateSyncType};
use crate::deployments::hybrid::{hybrid_deployment, INSTANCE_NAME_FORMAT};
use crate::k8s::K8sServiceConfigParams;

const NODE_IDS: [usize; 3] = [0, 1, 2];
const HTTP_SERVER_INGRESS_ALTERNATIVE_NAME: &str = "sn-alpha-test-upgrade.gateway-proxy.sw-dev.io";
const INGRESS_DOMAIN: &str = "sw-dev.io";
const SECRET_NAME_FORMAT: &str = "apollo-alpha-test-{}";
const NODE_NAMESPACE_FORMAT: &str = "apollo-alpha-test-{}";

const STARKNET_CONTRACT_ADDRESS: &str = "0x9b8A6361d204a0C1F93d5194763538057444d958";
const CHAIN_ID: &str = "SN_GOERLI";
const ETH_FEE_TOKEN_ADDRESS: &str =
    "0x7c07a3eec8ff611328722c3fc3e5d2e4ef2f60740c0bf86c756606036b74c16";
const STARKNET_GATEWAY_URL: &str = "https://feeder.sn-alpha-test-upgrade.gateway-proxy.sw-dev.io";
const STRK_FEE_TOKEN_ADDRESS: &str =
    "0x54a93d918d62b2fb62b25e77d9cb693bd277ab7e6fa236e53af263f1adb40e4";
const L1_STARTUP_HEIGHT_OVERRIDE: Option<BlockNumber> = None;
const STATE_SYNC_TYPE: StateSyncType = StateSyncType::Central;

const P2P_COMMUNICATION_TYPE: P2PCommunicationType = P2PCommunicationType::External;
const DEPLOYMENT_ENVIRONMENT: Environment = Environment::CloudK8s(CloudK8sEnvironment::UpgradeTest);

pub(crate) fn upgrade_test_hybrid_deployments() -> Vec<Deployment> {
    NODE_IDS
        .map(|i| {
            hybrid_deployment(
                i,
                P2P_COMMUNICATION_TYPE,
                DEPLOYMENT_ENVIRONMENT,
                &Template::new(INSTANCE_NAME_FORMAT),
                &Template::new(SECRET_NAME_FORMAT),
                DeploymentConfigOverride::new(
                    STARKNET_CONTRACT_ADDRESS,
                    CHAIN_ID,
                    ETH_FEE_TOKEN_ADDRESS,
                    Url::parse(STARKNET_GATEWAY_URL).expect("Invalid URL"),
                    STRK_FEE_TOKEN_ADDRESS,
                    L1_STARTUP_HEIGHT_OVERRIDE,
                    NODE_IDS.len(),
                    STATE_SYNC_TYPE,
                ),
                &Template::new(NODE_NAMESPACE_FORMAT),
                INGRESS_DOMAIN,
                HTTP_SERVER_INGRESS_ALTERNATIVE_NAME,
                Some(K8sServiceConfigParams::new(
                    Template::new(NODE_NAMESPACE_FORMAT).format(&[&i]),
                    INGRESS_DOMAIN.to_string(),
                    P2P_COMMUNICATION_TYPE,
                )),
            )
        })
        .to_vec()
}
