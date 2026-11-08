use std::collections::BTreeSet;

use serde::Serialize;
use strum::{AsRefStr, Display, EnumIter, EnumString, IntoEnumIterator};

use crate::deployment_definitions::ComponentConfigInService;
use crate::service::{NodeService, ServiceNameInner};

// Number of infra-required ports for a hybrid node service distribution.
pub const HYBRID_NODE_REQUIRED_PORTS_NUM: usize = 11;

#[derive(
    Clone, Copy, Debug, Display, EnumString, PartialEq, Eq, Hash, Serialize, AsRefStr, EnumIter,
)]
#[strum(serialize_all = "snake_case")]
pub enum HybridNodeServiceName {
    Committer,
    Core,    /* Comprises the batcher, class manager, consensus manager, proof manager, and
              * state sync. */
    Gateway, // Comprises the gateway and http server
    L1,      // Comprises the various l1 components.
    Mempool,
    SierraCompiler,
}

// Implement conversion from `HybridNodeServiceName` to `NodeService`
impl From<HybridNodeServiceName> for NodeService {
    fn from(service: HybridNodeServiceName) -> Self {
        NodeService::Hybrid(service)
    }
}

// TODO(Tsabary): per each service, update all values.
impl ServiceNameInner for HybridNodeServiceName {
    fn get_components_in_service(&self) -> BTreeSet<ComponentConfigInService> {
        let mut components = BTreeSet::new();
        match self {
            Self::Committer => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::Committer
                        | ComponentConfigInService::ConfigManager
                        | ComponentConfigInService::General
                        | ComponentConfigInService::MonitoringEndpoint => {
                            components.insert(component_config_in_service);
                        }
                        ComponentConfigInService::Batcher
                        | ComponentConfigInService::BaseLayer
                        | ComponentConfigInService::ClassManager
                        | ComponentConfigInService::ConsensusManager
                        | ComponentConfigInService::Gateway
                        | ComponentConfigInService::HttpServer
                        | ComponentConfigInService::L1GasPriceProvider
                        | ComponentConfigInService::L1GasPriceScraper
                        | ComponentConfigInService::L1EventsProvider
                        | ComponentConfigInService::L1EventsScraper
                        | ComponentConfigInService::Mempool
                        | ComponentConfigInService::MempoolP2p
                        | ComponentConfigInService::ProofManager
                        | ComponentConfigInService::SierraCompiler
                        | ComponentConfigInService::SignatureManager
                        | ComponentConfigInService::StateSync => {}
                    }
                }
            }
            Self::Core => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::Batcher
                        | ComponentConfigInService::ClassManager
                        | ComponentConfigInService::ConsensusManager
                        | ComponentConfigInService::ConfigManager
                        | ComponentConfigInService::General
                        | ComponentConfigInService::MonitoringEndpoint
                        | ComponentConfigInService::ProofManager
                        | ComponentConfigInService::SignatureManager
                        | ComponentConfigInService::StateSync => {
                            components.insert(component_config_in_service);
                        }
                        ComponentConfigInService::BaseLayer
                        | ComponentConfigInService::Committer
                        | ComponentConfigInService::Gateway
                        | ComponentConfigInService::HttpServer
                        | ComponentConfigInService::L1GasPriceProvider
                        | ComponentConfigInService::L1GasPriceScraper
                        | ComponentConfigInService::L1EventsProvider
                        | ComponentConfigInService::L1EventsScraper
                        | ComponentConfigInService::Mempool
                        | ComponentConfigInService::MempoolP2p
                        | ComponentConfigInService::SierraCompiler => {}
                    }
                }
            }
            Self::Gateway => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::ConfigManager
                        | ComponentConfigInService::Gateway
                        | ComponentConfigInService::HttpServer
                        | ComponentConfigInService::General
                        | ComponentConfigInService::MonitoringEndpoint => {
                            components.insert(component_config_in_service);
                        }
                        ComponentConfigInService::BaseLayer
                        | ComponentConfigInService::Batcher
                        | ComponentConfigInService::ClassManager
                        | ComponentConfigInService::Committer
                        | ComponentConfigInService::ConsensusManager
                        | ComponentConfigInService::L1GasPriceProvider
                        | ComponentConfigInService::L1GasPriceScraper
                        | ComponentConfigInService::L1EventsProvider
                        | ComponentConfigInService::L1EventsScraper
                        | ComponentConfigInService::Mempool
                        | ComponentConfigInService::MempoolP2p
                        | ComponentConfigInService::ProofManager
                        | ComponentConfigInService::SierraCompiler
                        | ComponentConfigInService::SignatureManager
                        | ComponentConfigInService::StateSync => {}
                    }
                }
            }
            Self::L1 => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::BaseLayer
                        | ComponentConfigInService::ConfigManager
                        | ComponentConfigInService::General
                        | ComponentConfigInService::L1GasPriceProvider
                        | ComponentConfigInService::L1GasPriceScraper
                        | ComponentConfigInService::L1EventsProvider
                        | ComponentConfigInService::L1EventsScraper
                        | ComponentConfigInService::MonitoringEndpoint => {
                            components.insert(component_config_in_service);
                        }
                        ComponentConfigInService::Batcher
                        | ComponentConfigInService::ClassManager
                        | ComponentConfigInService::Committer
                        | ComponentConfigInService::ConsensusManager
                        | ComponentConfigInService::Gateway
                        | ComponentConfigInService::HttpServer
                        | ComponentConfigInService::Mempool
                        | ComponentConfigInService::MempoolP2p
                        | ComponentConfigInService::ProofManager
                        | ComponentConfigInService::SierraCompiler
                        | ComponentConfigInService::SignatureManager
                        | ComponentConfigInService::StateSync => {}
                    }
                }
            }
            Self::Mempool => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::ConfigManager
                        | ComponentConfigInService::General
                        | ComponentConfigInService::Mempool
                        | ComponentConfigInService::MempoolP2p
                        | ComponentConfigInService::MonitoringEndpoint => {
                            components.insert(component_config_in_service);
                        }
                        ComponentConfigInService::BaseLayer
                        | ComponentConfigInService::Batcher
                        | ComponentConfigInService::ClassManager
                        | ComponentConfigInService::Committer
                        | ComponentConfigInService::ConsensusManager
                        | ComponentConfigInService::Gateway
                        | ComponentConfigInService::HttpServer
                        | ComponentConfigInService::L1GasPriceProvider
                        | ComponentConfigInService::L1GasPriceScraper
                        | ComponentConfigInService::L1EventsProvider
                        | ComponentConfigInService::L1EventsScraper
                        | ComponentConfigInService::ProofManager
                        | ComponentConfigInService::SierraCompiler
                        | ComponentConfigInService::SignatureManager
                        | ComponentConfigInService::StateSync => {}
                    }
                }
            }
            Self::SierraCompiler => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::ConfigManager
                        | ComponentConfigInService::General
                        | ComponentConfigInService::MonitoringEndpoint
                        | ComponentConfigInService::SierraCompiler => {
                            components.insert(component_config_in_service);
                        }
                        ComponentConfigInService::BaseLayer
                        | ComponentConfigInService::Batcher
                        | ComponentConfigInService::ClassManager
                        | ComponentConfigInService::Committer
                        | ComponentConfigInService::ConsensusManager
                        | ComponentConfigInService::Gateway
                        | ComponentConfigInService::HttpServer
                        | ComponentConfigInService::L1GasPriceProvider
                        | ComponentConfigInService::L1GasPriceScraper
                        | ComponentConfigInService::L1EventsProvider
                        | ComponentConfigInService::L1EventsScraper
                        | ComponentConfigInService::Mempool
                        | ComponentConfigInService::MempoolP2p
                        | ComponentConfigInService::ProofManager
                        | ComponentConfigInService::SignatureManager
                        | ComponentConfigInService::StateSync => {}
                    }
                }
            }
        }
        components
    }
}
