use apollo_infra_utils::template::Template;
use starknet_api::block::BlockNumber;
use url::Url;

use crate::config_override::DeploymentConfigOverride;
use crate::deployment::{Deployment, P2PCommunicationType};
use crate::deployment_definitions::{CloudK8sEnvironment, Environment, StateSyncType};
use crate::deployments::hybrid::{hybrid_deployment, INSTANCE_NAME_FORMAT};

const NODE_IDS: [usize; 3] = [0, 1, 2];
const HTTP_SERVER_INGRESS_ALTERNATIVE_NAME: &str = "apollo-stresstest-dev.sw-dev.io";
const INGRESS_DOMAIN: &str = "sw-dev.io";
const SECRET_NAME_FORMAT: &str = "apollo-stresstest-dev-{}";
const NODE_NAMESPACE_FORMAT: &str = "apollo-stresstest-dev-{}";

const STARKNET_CONTRACT_ADDRESS: &str = "0x4fA369fEBf0C574ea05EC12bC0e1Bc9Cd461Dd0f";
const CHAIN_ID: &str = "INTERNAL_STRESS_TEST";
const ETH_FEE_TOKEN_ADDRESS: &str =
    "0x7e813ecf3e7b3e14f07bd2f68cb4a3d12110e3c75ec5a63de3d2dacf1852904";
const STARKNET_GATEWAY_URL: &str = "http://feeder-gateway.starknet-0-14-0-stress-test-03:9713/";
const STRK_FEE_TOKEN_ADDRESS: &str =
    "0x2208cce4221df1f35943958340abc812aa79a8f6a533bff4ee00416d3d06cd6";
const L1_STARTUP_HEIGHT_OVERRIDE: Option<BlockNumber> = None;
const STATE_SYNC_TYPE: StateSyncType = StateSyncType::Central;

const P2P_COMMUNICATION_TYPE: P2PCommunicationType = P2PCommunicationType::Internal;
const DEPLOYMENT_ENVIRONMENT: Environment = Environment::CloudK8s(CloudK8sEnvironment::StressTest);

pub(crate) fn stress_test_hybrid_deployments() -> Vec<Deployment> {
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
                None,
            )
        })
        .to_vec()
}
