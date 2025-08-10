use apollo_infra_utils::template::Template;
use starknet_api::block::BlockNumber;
use url::Url;

use crate::config_override::DeploymentConfigOverride;
use crate::deployment::{Deployment, P2PCommunicationType};
use crate::deployment_definitions::{CloudK8sEnvironment, Environment, StateSyncType};
use crate::deployments::hybrid::{hybrid_deployment, INSTANCE_NAME_FORMAT};
use crate::k8s::K8sServiceConfigParams;

const NODE_IDS: [usize; 3] = [0, 1, 2];
const HTTP_SERVER_INGRESS_ALTERNATIVE_NAME: &str = "sn-mainnet-test-upgrade.gateway-proxy.sw-dev.io";
const INGRESS_DOMAIN: &str = "sw-dev.io";
const SECRET_NAME_FORMAT: &str = "apollo-mainnet-test-{}";
const NODE_NAMESPACE_FORMAT: &str = "apollo-mainnet-test-{}";

const STARKNET_CONTRACT_ADDRESS: &str = "0x9A24bE2884FE593dFA951eE19C751e3a7c89fECd";
const CHAIN_ID: &str = "SN_GOERLI";
const ETH_FEE_TOKEN_ADDRESS: &str =
    "0x4475715fa6768670bb310eab072171856c94c1a04fa78be2370513aa2a87dc4";
const STARKNET_GATEWAY_URL: &str = "https://feeder.sn-mainnet-test-upgrade.gateway-proxy.sw-dev.io";
const STRK_FEE_TOKEN_ADDRESS: &str =
    "0x6cd5b5125491c4bccec3d3b8635cbd98542c1d91a134541eca6e108cf0639f6";
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
