use serde::Serialize;
use strum::EnumIter;
use strum_macros::{AsRefStr, Display};

#[cfg(test)]
#[path = "deployment_definitions_test.rs"]
mod deployment_definitions_test;

pub(crate) const CONFIG_BASE_DIR: &str = "crates/apollo_deployments/resources/";
pub(crate) const RETRIES_FOR_L1_SERVICES: usize = 0;

const BASE_APP_CONFIGS_DIR_PATH: &str = "crates/apollo_deployments/resources/app_configs";

#[derive(
    Hash, Clone, Debug, Display, Serialize, PartialEq, Eq, PartialOrd, Ord, EnumIter, AsRefStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum ComponentConfigInService {
    BaseLayer,
    Batcher,
    ClassManager,
    Committer,
    ConfigManager,
    ConsensusManager,
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
            // Signature manager does not have a separate config sub-struct in
            // `SequencerNodeConfig`. Keep this empty to avoid generating
            // `signature_manager_config.#is_none` flags.
            // TODO(Nadin): TAL add refactor this temp fix.
            ComponentConfigInService::SignatureManager => vec![],
            _ => {
                let base = self.as_ref();
                vec![format!("{base}_config")]
            }
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
