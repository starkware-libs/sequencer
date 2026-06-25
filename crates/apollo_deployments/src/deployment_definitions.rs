use serde::Serialize;
use strum::{Display, EnumIter};

#[cfg(test)]
#[path = "deployment_definitions_test.rs"]
mod deployment_definitions_test;

pub(crate) const RETRIES_FOR_L1_SERVICES: usize = 0;

#[derive(Hash, Clone, Debug, Display, Serialize, PartialEq, Eq, PartialOrd, Ord, EnumIter)]
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
    L1EventsProvider,
    L1EventsScraper,
    Mempool,
    MempoolP2p,
    MonitoringEndpoint,
    ProofManager,
    SierraCompiler,
    SignatureManager,
    StateSync,
}
