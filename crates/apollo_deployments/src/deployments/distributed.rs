use std::collections::BTreeSet;

use serde::Serialize;
use strum::{AsRefStr, Display, EnumIter, EnumString, IntoEnumIterator};

use crate::deployment_definitions::ComponentConfigInService;
use crate::service::{NodeService, ServiceNameInner};

// Number of infra-required ports for a distributed node service distribution.
pub const DISTRIBUTED_NODE_REQUIRED_PORTS_NUM: usize = 11;

// TODO(Tsabary): define consts and functions whenever relevant.

#[derive(
    Clone, Copy, Debug, Display, EnumString, PartialEq, Eq, Hash, Serialize, AsRefStr, EnumIter,
)]
#[strum(serialize_all = "snake_case")]
pub enum DistributedNodeServiceName {
    Batcher,
    ClassManager,
    Committer,
    ConsensusManager,
    HttpServer,
    Gateway,
    L1,
    ProofManager,
    Mempool,
    SierraCompiler,
    SignatureManager,
    StateSync,
}

// Implement conversion from `DistributedNodeServiceName` to `NodeService`
impl From<DistributedNodeServiceName> for NodeService {
    fn from(service: DistributedNodeServiceName) -> Self {
        Self::Distributed(service)
    }
}

// TODO(Tsabary): per each service, update all values.
impl ServiceNameInner for DistributedNodeServiceName {
    // TODO(Tsabary): verify that each service runs the components it should.
    fn get_components_in_service(&self) -> BTreeSet<ComponentConfigInService> {
        let mut components = BTreeSet::new();
        match self {
            Self::Batcher => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::Batcher
                        | ComponentConfigInService::ConfigManager
                        | ComponentConfigInService::General
                        | ComponentConfigInService::MonitoringEndpoint => {
                            components.insert(component_config_in_service);
                        }
                        ComponentConfigInService::BaseLayer
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
                        | ComponentConfigInService::SierraCompiler
                        | ComponentConfigInService::SignatureManager
                        | ComponentConfigInService::StateSync => {}
                    }
                }
            }
            Self::Committer => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::Committer
                        | ComponentConfigInService::ConfigManager
                        | ComponentConfigInService::General
                        | ComponentConfigInService::MonitoringEndpoint => {
                            components.insert(component_config_in_service);
                        }
                        ComponentConfigInService::BaseLayer
                        | ComponentConfigInService::Batcher
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
            Self::ClassManager => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::ClassManager
                        | ComponentConfigInService::ConfigManager
                        | ComponentConfigInService::General
                        | ComponentConfigInService::MonitoringEndpoint => {
                            components.insert(component_config_in_service);
                        }
                        ComponentConfigInService::BaseLayer
                        | ComponentConfigInService::Batcher
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
                        | ComponentConfigInService::SierraCompiler
                        | ComponentConfigInService::SignatureManager
                        | ComponentConfigInService::StateSync => {}
                    }
                }
            }
            Self::ConsensusManager => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::ConsensusManager
                        | ComponentConfigInService::ConfigManager
                        | ComponentConfigInService::General
                        | ComponentConfigInService::MonitoringEndpoint => {
                            components.insert(component_config_in_service);
                        }
                        ComponentConfigInService::BaseLayer
                        | ComponentConfigInService::Batcher
                        | ComponentConfigInService::ClassManager
                        | ComponentConfigInService::Committer
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
            Self::HttpServer => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::ConfigManager
                        | ComponentConfigInService::General
                        | ComponentConfigInService::HttpServer
                        | ComponentConfigInService::MonitoringEndpoint => {
                            components.insert(component_config_in_service);
                        }
                        ComponentConfigInService::BaseLayer
                        | ComponentConfigInService::Batcher
                        | ComponentConfigInService::ClassManager
                        | ComponentConfigInService::Committer
                        | ComponentConfigInService::ConsensusManager
                        | ComponentConfigInService::Gateway
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
            Self::Gateway => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::ConfigManager
                        | ComponentConfigInService::Gateway
                        | ComponentConfigInService::General
                        | ComponentConfigInService::MonitoringEndpoint => {
                            components.insert(component_config_in_service);
                        }
                        ComponentConfigInService::BaseLayer
                        | ComponentConfigInService::Batcher
                        | ComponentConfigInService::ClassManager
                        | ComponentConfigInService::Committer
                        | ComponentConfigInService::ConsensusManager
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
            Self::ProofManager => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::ConfigManager
                        | ComponentConfigInService::General
                        | ComponentConfigInService::MonitoringEndpoint
                        | ComponentConfigInService::ProofManager => {
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
            Self::SignatureManager => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::ConfigManager
                        | ComponentConfigInService::General
                        | ComponentConfigInService::MonitoringEndpoint
                        | ComponentConfigInService::SignatureManager => {
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
                        | ComponentConfigInService::SierraCompiler
                        | ComponentConfigInService::StateSync => {}
                    }
                }
            }
            Self::StateSync => {
                for component_config_in_service in ComponentConfigInService::iter() {
                    match component_config_in_service {
                        ComponentConfigInService::ConfigManager
                        | ComponentConfigInService::General
                        | ComponentConfigInService::MonitoringEndpoint
                        | ComponentConfigInService::StateSync => {
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
                        | ComponentConfigInService::SierraCompiler
                        | ComponentConfigInService::SignatureManager => {}
                    }
                }
            }
        }
        components
    }
}
