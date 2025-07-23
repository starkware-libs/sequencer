use apollo_infra_utils::template::Template;
use starknet_api::block::BlockNumber;
use url::Url;

use crate::config_override::DeploymentConfigOverride;
use crate::deployment::{Deployment, P2PCommunicationType};
use crate::deployment_definitions::{CloudK8sEnvironment, Environment, StateSyncType};
use crate::deployments::hybrid::{hybrid_deployment, INSTANCE_NAME_FORMAT};
use crate::k8s::K8sServiceConfigParams;

const NODE_IDS: [usize; 3] = [0, 1, 2];
const HTTP_SERVER_INGRESS_ALTERNATIVE_NAME: &str = "alpha-mainnet.starknet.io";
const INGRESS_DOMAIN: &str = "starknet.io";
const SECRET_NAME_FORMAT: &str = "apollo-mainnet-{}";
const NODE_NAMESPACE_FORMAT: &str = "apollo-mainnet-{}";

const STARKNET_CONTRACT_ADDRESS: &str = "0xc662c410C0ECf747543f5bA90660f6ABeBD9C8c4";
const CHAIN_ID: &str = "SN_MAIN";
const ETH_FEE_TOKEN_ADDRESS: &str =
    "0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7";
const STARKNET_GATEWAY_URL: &str = "https://feeder.alpha-mainnet.starknet.io";
const STRK_FEE_TOKEN_ADDRESS: &str =
    "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d";
const L1_STARTUP_HEIGHT_OVERRIDE: Option<BlockNumber> = None;
const STATE_SYNC_TYPE: StateSyncType = StateSyncType::Central;

const P2P_COMMUNICATION_TYPE: P2PCommunicationType = P2PCommunicationType::External;
const DEPLOYMENT_ENVIRONMENT: Environment = Environment::CloudK8s(CloudK8sEnvironment::Mainnet);

pub(crate) fn mainnet_hybrid_deployments() -> Vec<Deployment> {
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
