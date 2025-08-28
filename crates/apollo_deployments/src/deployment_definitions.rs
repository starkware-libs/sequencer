use std::fmt::{Display, Formatter, Result};
use std::fs::read_to_string;
use std::path::PathBuf;

use alloy::primitives::Address as EthereumContractAddress;
use apollo_http_server::config::HTTP_SERVER_PORT;
use apollo_infra_utils::template::Template;
use apollo_monitoring_endpoint::config::MONITORING_ENDPOINT_DEFAULT_PORT;
use serde::{Deserialize, Serialize};
use serde_json::from_str;
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use strum::{EnumDiscriminants, EnumIter, IntoEnumIterator};
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

const BATCHER_PORT: u16 = 55000;
const CLASS_MANAGER_PORT: u16 = 55001;
const CONSENSUS_P2P_PORT: u16 = 53080;
const GATEWAY_PORT: u16 = 55002;
const L1_ENDPOINT_MONITOR_PORT: u16 = 55005;
const L1_GAS_PRICE_PROVIDER_PORT: u16 = 55003;
const L1_PROVIDER_PORT: u16 = 55004;
const MEMPOOL_PORT: u16 = 55006;
const MEMPOOL_P2P_PORT: u16 = 53200;
const SIERRA_COMPILER_PORT: u16 = 55007;
const SIGNATURE_MANAGER_PORT: u16 = 55008;
const STATE_SYNC_PORT: u16 = 55009;

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

const BASE_APP_CONFIGS_DIR_PATH: &str = "crates/apollo_deployments/resources/app_configs";

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
    pub num_validators: usize,
    pub http_server_ingress_alternative_name: String,
    pub ingress_domain: String,
    pub secret_name_format: Template,
    pub node_namespace_format: Template,
    pub starknet_contract_address: EthereumContractAddress,
    pub chain_id_string: String,
    pub eth_fee_token_address: ContractAddress,
    pub starknet_gateway_url: Url,
    pub strk_fee_token_address: ContractAddress,
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

#[derive(Clone, Copy, Debug, EnumIter, Display, Serialize, Ord, PartialEq, Eq, PartialOrd)]
pub enum BusinessLogicServicePort {
    ConsensusP2p,
    HttpServer,
    MempoolP2p,
    MonitoringEndpoint,
}

impl BusinessLogicServicePort {
    pub fn get_port(&self) -> u16 {
        match self {
            BusinessLogicServicePort::ConsensusP2p => CONSENSUS_P2P_PORT,
            BusinessLogicServicePort::HttpServer => HTTP_SERVER_PORT,
            BusinessLogicServicePort::MempoolP2p => MEMPOOL_P2P_PORT,
            BusinessLogicServicePort::MonitoringEndpoint => MONITORING_ENDPOINT_DEFAULT_PORT,
        }
    }
}

// TODO(Nadin): Integrate this logic with `ComponentConfigInService` once the merge from main-14.0
// is complete.
#[derive(Clone, Copy, Debug, EnumIter, Display, Serialize, Ord, PartialEq, Eq, PartialOrd)]
pub enum InfraServicePort {
    Batcher,
    ClassManager,
    Gateway,
    L1EndpointMonitor,
    L1GasPriceProvider,
    L1Provider,
    Mempool,
    SierraCompiler,
    SignatureManager,
    StateSync,
}

impl InfraServicePort {
    pub fn get_port(&self) -> u16 {
        match self {
            InfraServicePort::Batcher => BATCHER_PORT,
            InfraServicePort::ClassManager => CLASS_MANAGER_PORT,
            InfraServicePort::Gateway => GATEWAY_PORT,
            InfraServicePort::L1EndpointMonitor => L1_ENDPOINT_MONITOR_PORT,
            InfraServicePort::L1GasPriceProvider => L1_GAS_PRICE_PROVIDER_PORT,
            InfraServicePort::L1Provider => L1_PROVIDER_PORT,
            InfraServicePort::Mempool => MEMPOOL_PORT,
            InfraServicePort::SierraCompiler => SIERRA_COMPILER_PORT,
            InfraServicePort::SignatureManager => SIGNATURE_MANAGER_PORT,
            InfraServicePort::StateSync => STATE_SYNC_PORT,
        }
    }
}

#[derive(Clone, Copy, Debug, Display, Ord, PartialEq, Eq, PartialOrd, EnumDiscriminants)]
pub enum ServicePort {
    Infra(InfraServicePort),
    BusinessLogic(BusinessLogicServicePort),
}

impl serde::Serialize for ServicePort {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            ServicePort::Infra(port) => serde::Serialize::serialize(port, serializer),
            ServicePort::BusinessLogic(port) => serde::Serialize::serialize(port, serializer),
        }
    }
}

impl ServicePort {
    pub fn get_port(&self) -> u16 {
        match self {
            ServicePort::Infra(inner) => inner.get_port(),
            ServicePort::BusinessLogic(inner) => inner.get_port(),
        }
    }

    pub fn iter() -> impl Iterator<Item = ServicePort> {
        InfraServicePort::iter()
            .map(ServicePort::Infra)
            .chain(BusinessLogicServicePort::iter().map(ServicePort::BusinessLogic))
    }
}

#[derive(Clone, Debug, Display, Serialize, PartialEq, Eq, PartialOrd, Ord, EnumIter)]
pub enum ComponentConfigInService {
    BaseLayer,
    Batcher,
    ClassManager,
    Consensus,
    General, // General configs that are not specific to any service, e.g., pointer targets.
    Gateway,
    HttpServer,
    L1EndpointMonitor,
    L1GasPriceProvider,
    L1GasPriceScraper,
    L1Provider,
    L1Scraper,
    Mempool,
    MempoolP2p,
    MonitoringEndpoint,
    SierraCompiler,
    SignatureManager,
    StateSync,
}

impl ComponentConfigInService {
    pub fn get_component_config_names(&self) -> Vec<String> {
        match self {
            ComponentConfigInService::BaseLayer => vec!["base_layer_config".to_string()],
            ComponentConfigInService::Batcher => vec!["batcher_config".to_string()],
            ComponentConfigInService::ClassManager => vec!["class_manager_config".to_string()],
            ComponentConfigInService::Consensus => vec!["consensus_manager_config".to_string()],
            ComponentConfigInService::General => vec![
                "revert_config".to_string(),
                "versioned_constants_overrides_config".to_string(),
                "validate_resource_bounds_config".to_string(),
            ],
            ComponentConfigInService::Gateway => vec!["gateway_config".to_string()],
            ComponentConfigInService::HttpServer => vec!["http_server_config".to_string()],
            ComponentConfigInService::L1EndpointMonitor => {
                vec!["l1_endpoint_monitor_config".to_string()]
            }
            ComponentConfigInService::L1GasPriceProvider => {
                vec!["l1_gas_price_provider_config".to_string()]
            }
            ComponentConfigInService::L1GasPriceScraper => {
                vec!["l1_gas_price_scraper_config".to_string()]
            }
            ComponentConfigInService::L1Provider => vec!["l1_provider_config".to_string()],
            ComponentConfigInService::L1Scraper => vec!["l1_scraper_config".to_string()],
            ComponentConfigInService::Mempool => vec!["mempool_config".to_string()],
            ComponentConfigInService::MempoolP2p => vec!["mempool_p2p_config".to_string()],
            ComponentConfigInService::MonitoringEndpoint => {
                vec!["monitoring_endpoint_config".to_string()]
            }
            ComponentConfigInService::SierraCompiler => vec!["sierra_compiler_config".to_string()],
            // Signature manager does not have a separate config sub-struct in
            // `SequencerNodeConfig`. Keep this empty to avoid generating
            // `signature_manager_config.#is_none` flags.
            // TODO(Nadin): TAL add refactor this temp fix.
            ComponentConfigInService::SignatureManager => vec![],
            ComponentConfigInService::StateSync => vec!["state_sync_config".to_string()],
        }
    }

    pub fn get_component_config_file_paths(&self) -> Vec<String> {
        self.get_component_config_names()
            .into_iter()
            .map(|name| format!("{BASE_APP_CONFIGS_DIR_PATH}/{name}.json"))
            .collect()
    }
}
