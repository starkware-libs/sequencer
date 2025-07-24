use std::fmt::{Display, Formatter, Result};
use std::fs::read_to_string;
use std::path::PathBuf;

use apollo_infra_utils::template::Template;
use serde::{Deserialize, Serialize};
use serde_json::from_str;
use starknet_api::block::BlockNumber;
use strum_macros::{Display, EnumString};
use url::Url;

use crate::deployment::{Deployment, P2PCommunicationType};
use crate::deployment_definitions::testing::system_test_deployments;
use crate::deployment_definitions::upgrade_test::upgrade_test_hybrid_deployments;
use crate::deployments::hybrid::load_and_create_hybrid_deployments;

#[cfg(test)]
#[path = "deployment_definitions_test.rs"]
mod deployment_definitions_test;

mod testing;
mod upgrade_test;

type DeploymentFn = fn() -> Vec<Deployment>;

pub const DEPLOYMENTS: &[DeploymentFn] = &[
    || load_and_create_hybrid_deployments(POTC2_DEPLOYMENT_INPUTS_PATH),
    || load_and_create_hybrid_deployments(MAINNET_DEPLOYMENT_INPUTS_PATH),
    || load_and_create_hybrid_deployments(INTEGRATION_DEPLOYMENT_INPUTS_PATH),
    || load_and_create_hybrid_deployments(TESTNET_DEPLOYMENT_INPUTS_PATH),
    || load_and_create_hybrid_deployments(STRESS_TEST_DEPLOYMENT_INPUTS_PATH),
    system_test_deployments,
    upgrade_test_hybrid_deployments, // TODO(Tsabary): this env is deprecated, remove it.
];

pub(crate) const CONFIG_BASE_DIR: &str = "crates/apollo_deployments/resources/";
pub(crate) const DEPLOYMENT_CONFIG_DIR_NAME: &str = "deployments/";

const POTC2_DEPLOYMENT_INPUTS_PATH: &str =
    "crates/apollo_deployments/resources/deployment_inputs/potc2_sepolia.json";
const MAINNET_DEPLOYMENT_INPUTS_PATH: &str =
    "crates/apollo_deployments/resources/deployment_inputs/mainnet.json";
const INTEGRATION_DEPLOYMENT_INPUTS_PATH: &str =
    "crates/apollo_deployments/resources/deployment_inputs/sepolia_integration.json";
const TESTNET_DEPLOYMENT_INPUTS_PATH: &str =
    "crates/apollo_deployments/resources/deployment_inputs/sepolia_testnet.json";
const STRESS_TEST_DEPLOYMENT_INPUTS_PATH: &str =
    "crates/apollo_deployments/resources/deployment_inputs/stress_test.json";

#[derive(Debug, Deserialize)]
pub struct DeploymentInputs {
    pub node_ids: Vec<usize>,
    pub http_server_ingress_alternative_name: String,
    pub ingress_domain: String,
    pub secret_name_format: Template,
    pub node_namespace_format: Template,
    pub starknet_contract_address: String, /* TODO(Tsabary): should be an Eth address, currently
                                            * only enforced at config loading. */
    pub chain_id_string: String,
    pub eth_fee_token_address: String, /* TODO(Tsabary): should be a Starknet address, currently
                                        * only enforced at config loading. */
    pub starknet_gateway_url: Url,
    pub strk_fee_token_address: String, /* TODO(Tsabary): should be a Starknet address,
                                         * currently only enforced at config loading. */
    pub l1_startup_height_override: Option<BlockNumber>,
    pub state_sync_type: StateSyncType,
    pub p2p_communication_type: P2PCommunicationType,
    pub deployment_environment: Environment,
    pub requires_k8s_service_config_params: bool,
}

impl DeploymentInputs {
    pub fn load_from_file(path: PathBuf) -> DeploymentInputs {
        // Read the file into a string
        let data = read_to_string(path).expect("Failed to read deployment input JSON file");

        // Parse JSON into the DeploymentInputs struct
        from_str(&data).expect("Should be able to parse deployment input JSON")
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Environment {
    CloudK8s(CloudK8sEnvironment),
    #[serde(rename = "local_k8s")]
    LocalK8s,
}

impl Display for Environment {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Environment::CloudK8s(e) => write!(f, "{e}"),
            Environment::LocalK8s => write!(f, "testing"),
        }
    }
}

#[derive(EnumString, Clone, Display, PartialEq, Debug, Serialize, Deserialize)]
#[strum(serialize_all = "snake_case")]
pub enum CloudK8sEnvironment {
    Potc2,
    Mainnet,
    SepoliaIntegration,
    SepoliaTestnet,
    #[strum(serialize = "stress_test")]
    StressTest,
    #[strum(serialize = "upgrade_test")]
    UpgradeTest,
}

impl Environment {
    pub fn env_dir_path(&self) -> PathBuf {
        let env_str = match self {
            Environment::CloudK8s(env) => env.to_string(),
            Environment::LocalK8s => "testing".to_string(),
        };
        PathBuf::from(CONFIG_BASE_DIR).join(DEPLOYMENT_CONFIG_DIR_NAME).join(env_str)
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct StateSyncConfig {
    #[serde(rename = "state_sync_config.central_sync_client_config.#is_none")]
    state_sync_config_central_sync_client_config_is_none: bool,
    #[serde(rename = "state_sync_config.p2p_sync_client_config.#is_none")]
    state_sync_config_p2p_sync_client_config_is_none: bool,
    #[serde(rename = "state_sync_config.network_config.#is_none")]
    state_sync_config_network_config_is_none: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum StateSyncType {
    Central,
    P2P,
}

impl StateSyncType {
    pub fn get_state_sync_config(&self) -> StateSyncConfig {
        match self {
            StateSyncType::Central => StateSyncConfig {
                state_sync_config_central_sync_client_config_is_none: false,
                state_sync_config_p2p_sync_client_config_is_none: true,
                state_sync_config_network_config_is_none: true,
            },
            StateSyncType::P2P => StateSyncConfig {
                state_sync_config_central_sync_client_config_is_none: true,
                state_sync_config_p2p_sync_client_config_is_none: false,
                state_sync_config_network_config_is_none: false,
            },
        }
    }
}

#[derive(Clone, Debug, Display, Serialize, PartialEq)]
pub enum ServicePort {
    Batcher,
    ClassManager,
    Gateway,
    L1EndpointMonitor,
    L1GasPriceProvider,
    L1Provider,
    Mempool,
    MempoolP2p,
    SierraCompiler,
    StateSync,
    HttpServer,
    MonitoringEndpoint,
}
