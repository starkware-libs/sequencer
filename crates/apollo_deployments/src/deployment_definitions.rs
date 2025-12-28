use apollo_http_server_config::config::HTTP_SERVER_PORT;
use apollo_monitoring_endpoint_config::config::MONITORING_ENDPOINT_DEFAULT_PORT;
use serde::{Deserialize, Serialize};
use strum::{EnumDiscriminants, EnumIter, IntoEnumIterator};
use strum_macros::Display;

#[cfg(test)]
#[path = "deployment_definitions_test.rs"]
mod deployment_definitions_test;

const BATCHER_PORT: u16 = 55000;
const CLASS_MANAGER_PORT: u16 = 55001;
const COMMITTER_PORT: u16 = 55011;
pub(crate) const CONSENSUS_P2P_PORT: u16 = 53080;
const GATEWAY_PORT: u16 = 55002;
const L1_GAS_PRICE_PROVIDER_PORT: u16 = 55003;
const L1_PROVIDER_PORT: u16 = 55004;
const MEMPOOL_PORT: u16 = 55006;
pub(crate) const MEMPOOL_P2P_PORT: u16 = 53200;
const SIERRA_COMPILER_PORT: u16 = 55007;
const SIGNATURE_MANAGER_PORT: u16 = 55008;
const STATE_SYNC_PORT: u16 = 55009;

pub(crate) const CONFIG_BASE_DIR: &str = "crates/apollo_deployments/resources/";

const BASE_APP_CONFIGS_DIR_PATH: &str = "crates/apollo_deployments/resources/app_configs";

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
    Committer,
    Gateway,
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
            InfraServicePort::Committer => COMMITTER_PORT,
            InfraServicePort::Gateway => GATEWAY_PORT,
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

#[derive(Hash, Clone, Debug, Display, Serialize, PartialEq, Eq, PartialOrd, Ord, EnumIter)]
pub enum ComponentConfigInService {
    BaseLayer,
    Batcher,
    ClassManager,
    Committer,
    ConfigManager,
    Consensus,
    General, // General configs that are not specific to any service, e.g., pointer targets.
    Gateway,
    HttpServer,
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

// TODO(Tsabary): consider moving from `vec` to a single element.
impl ComponentConfigInService {
    pub fn get_component_config_names(&self) -> Vec<String> {
        match self {
            ComponentConfigInService::BaseLayer => vec!["base_layer_config".to_string()],
            ComponentConfigInService::Batcher => vec!["batcher_config".to_string()],
            ComponentConfigInService::ClassManager => vec!["class_manager_config".to_string()],
            ComponentConfigInService::Committer => vec!["committer_config".to_string()],
            ComponentConfigInService::ConfigManager => vec!["config_manager_config".to_string()],
            ComponentConfigInService::Consensus => vec!["consensus_manager_config".to_string()],
            ComponentConfigInService::General => vec!["general_config".to_string()],
            ComponentConfigInService::Gateway => vec!["gateway_config".to_string()],
            ComponentConfigInService::HttpServer => vec!["http_server_config".to_string()],
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

    pub fn get_replacer_component_config_file_paths(&self) -> Vec<String> {
        self.get_component_config_names()
            .into_iter()
            .map(|name| format!("{BASE_APP_CONFIGS_DIR_PATH}/replacer_{name}.json"))
            .collect()
    }
}
