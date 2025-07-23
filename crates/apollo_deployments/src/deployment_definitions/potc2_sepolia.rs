use apollo_infra_utils::template::Template;
use starknet_api::block::BlockNumber;
use url::Url;

use crate::config_override::DeploymentConfigOverride;
use crate::deployment::{Deployment, P2PCommunicationType};
use crate::deployment_definitions::{CloudK8sEnvironment, Environment, StateSyncType};
use crate::deployments::hybrid::{hybrid_deployment, INSTANCE_NAME_FORMAT};
use crate::k8s::K8sServiceConfigParams;

const NODE_IDS: [usize; 3] = [0, 1, 2];
const HTTP_SERVER_INGRESS_ALTERNATIVE_NAME: &str = "potc-mock-sepolia.starknet.io";
const INGRESS_DOMAIN: &str = "starknet.io";
const SECRET_NAME_FORMAT: &str = "apollo-potc-2-sepolia-mock-sharp-{}";
const NODE_NAMESPACE_FORMAT: &str = "apollo-potc-2-sepolia-mock-sharp-{}";

const STARKNET_CONTRACT_ADDRESS: &str = "0xd8A5518cf4AC3ECD3b4cec772478109679a73E78";
const CHAIN_ID: &str = "PRIVATE_SN_POTC_MOCK_SEPOLIA";
const ETH_FEE_TOKEN_ADDRESS: &str =
    "0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7";
const STARKNET_GATEWAY_URL: &str = "https://feeder.potc-mock-sepolia-fgw.starknet.io/";
const STRK_FEE_TOKEN_ADDRESS: &str =
    "0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7";
const L1_STARTUP_HEIGHT_OVERRIDE: Option<BlockNumber> = None;
const STATE_SYNC_TYPE: StateSyncType = StateSyncType::Central;

const P2P_COMMUNICATION_TYPE: P2PCommunicationType = P2PCommunicationType::Internal;
const DEPLOYMENT_ENVIRONMENT: Environment = Environment::CloudK8s(CloudK8sEnvironment::Potc2);

pub(crate) fn potc2_sepolia_hybrid_deployments() -> Vec<Deployment> {
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
