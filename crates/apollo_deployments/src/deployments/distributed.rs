use std::collections::BTreeSet;

use serde::Serialize;
use strum::{AsRefStr, Display, EnumIter, EnumString, IntoEnumIterator};

<<<<<<< HEAD
use crate::deployment_definitions::ComponentConfigInService;
use crate::service::{NodeService, ServiceNameInner};
||||||| 01e4b43fa6
use crate::deployment_definitions::{ComponentConfigInService, RETRIES_FOR_L1_SERVICES};
use crate::scale_policy::ScalePolicy;
use crate::service::{GetComponentConfigs, NodeService, ServiceNameInner};
use crate::utils::InfraPortAllocator;
=======
use crate::deployment_definitions::{ComponentConfigInService, RETRIES_FOR_L1_SERVICES};
use crate::scale_policy::ScalePolicy;
use crate::service::{
    GetComponentConfigs,
    NodeService,
    ServiceNameInner,
    COMMITTER_MAX_REQUEST_BODY_BYTES,
    COMMITTER_MAX_RESPONSE_BODY_BYTES,
};
use crate::utils::InfraPortAllocator;
>>>>>>> origin/main-v0.14.4

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

<<<<<<< HEAD
||||||| 01e4b43fa6
impl GetComponentConfigs for DistributedNodeServiceName {
    fn get_component_configs(ports: Option<Vec<u16>>) -> HashMap<NodeService, ComponentConfig> {
        let mut infra_port_allocator =
            InfraPortAllocator::new(ports, DISTRIBUTED_NODE_REQUIRED_PORTS_NUM);
        let batcher = Self::Batcher.component_config_pair(infra_port_allocator.next());
        let class_manager = Self::ClassManager.component_config_pair(infra_port_allocator.next());
        let committer = Self::Committer.component_config_pair(infra_port_allocator.next());
        let gateway = Self::Gateway.component_config_pair(infra_port_allocator.next());
        let l1_gas_price_provider = Self::L1.component_config_pair(infra_port_allocator.next());
        let l1_events_provider = Self::L1.component_config_pair(infra_port_allocator.next());
        let mempool = Self::Mempool.component_config_pair(infra_port_allocator.next());
        let proof_manager = Self::ProofManager.component_config_pair(infra_port_allocator.next());
        let sierra_compiler =
            Self::SierraCompiler.component_config_pair(infra_port_allocator.next());
        let signature_manager =
            Self::SignatureManager.component_config_pair(infra_port_allocator.next());
        let state_sync = Self::StateSync.component_config_pair(infra_port_allocator.next());

        let mut component_config_map = HashMap::<NodeService, ComponentConfig>::new();
        for inner_service_name in Self::iter() {
            let component_config = match inner_service_name {
                Self::Batcher => get_batcher_component_config(
                    batcher.local(),
                    class_manager.remote(),
                    committer.remote(),
                    l1_events_provider.remote(),
                    mempool.remote(),
                    proof_manager.remote(),
                ),
                Self::Committer => {
                    get_committer_component_config(committer.local(), batcher.remote())
                }
                Self::ClassManager => get_class_manager_component_config(
                    class_manager.local(),
                    sierra_compiler.remote(),
                ),
                Self::ConsensusManager => get_consensus_manager_component_config(
                    batcher.remote(),
                    class_manager.remote(),
                    l1_gas_price_provider.remote(),
                    proof_manager.remote(),
                    state_sync.remote(),
                    signature_manager.remote(),
                ),
                Self::HttpServer => get_http_server_component_config(gateway.remote()),
                Self::Gateway => get_gateway_component_config(
                    gateway.local(),
                    class_manager.remote(),
                    mempool.remote(),
                    proof_manager.remote(),
                    state_sync.remote(),
                ),
                Self::L1 => get_l1_component_config(
                    l1_gas_price_provider.local(),
                    l1_events_provider.local(),
                    state_sync.remote(),
                    batcher.remote(),
                ),
                Self::Mempool => get_mempool_component_config(
                    mempool.local(),
                    class_manager.remote(),
                    gateway.remote(),
                    proof_manager.remote(),
                ),
                Self::ProofManager => get_proof_manager_component_config(proof_manager.local()),
                Self::SierraCompiler => {
                    get_sierra_compiler_component_config(sierra_compiler.local())
                }
                Self::SignatureManager => {
                    get_signature_manager_component_config(signature_manager.local())
                }
                Self::StateSync => {
                    get_state_sync_component_config(state_sync.local(), class_manager.remote())
                }
            };
            let node_service = inner_service_name.into();
            component_config_map.insert(node_service, component_config);
        }
        component_config_map
    }
}

=======
impl GetComponentConfigs for DistributedNodeServiceName {
    fn get_component_configs(ports: Option<Vec<u16>>) -> HashMap<NodeService, ComponentConfig> {
        let mut infra_port_allocator =
            InfraPortAllocator::new(ports, DISTRIBUTED_NODE_REQUIRED_PORTS_NUM);
        let batcher = Self::Batcher.component_config_pair(infra_port_allocator.next());
        let class_manager = Self::ClassManager.component_config_pair(infra_port_allocator.next());
        let committer =
            Self::Committer.component_config_pair(infra_port_allocator.next()).with_max_body_bytes(
                COMMITTER_MAX_REQUEST_BODY_BYTES,
                COMMITTER_MAX_RESPONSE_BODY_BYTES,
            );
        let gateway = Self::Gateway.component_config_pair(infra_port_allocator.next());
        let l1_gas_price_provider = Self::L1.component_config_pair(infra_port_allocator.next());
        let l1_events_provider = Self::L1.component_config_pair(infra_port_allocator.next());
        let mempool = Self::Mempool.component_config_pair(infra_port_allocator.next());
        let proof_manager = Self::ProofManager.component_config_pair(infra_port_allocator.next());
        let sierra_compiler =
            Self::SierraCompiler.component_config_pair(infra_port_allocator.next());
        let signature_manager =
            Self::SignatureManager.component_config_pair(infra_port_allocator.next());
        let state_sync = Self::StateSync.component_config_pair(infra_port_allocator.next());

        let mut component_config_map = HashMap::<NodeService, ComponentConfig>::new();
        for inner_service_name in Self::iter() {
            let component_config = match inner_service_name {
                Self::Batcher => get_batcher_component_config(
                    batcher.local(),
                    class_manager.remote(),
                    committer.remote(),
                    l1_events_provider.remote(),
                    mempool.remote(),
                    proof_manager.remote(),
                ),
                Self::Committer => {
                    get_committer_component_config(committer.local(), batcher.remote())
                }
                Self::ClassManager => get_class_manager_component_config(
                    class_manager.local(),
                    sierra_compiler.remote(),
                ),
                Self::ConsensusManager => get_consensus_manager_component_config(
                    batcher.remote(),
                    class_manager.remote(),
                    l1_gas_price_provider.remote(),
                    proof_manager.remote(),
                    state_sync.remote(),
                    signature_manager.remote(),
                ),
                Self::HttpServer => get_http_server_component_config(gateway.remote()),
                Self::Gateway => get_gateway_component_config(
                    gateway.local(),
                    class_manager.remote(),
                    mempool.remote(),
                    proof_manager.remote(),
                    state_sync.remote(),
                ),
                Self::L1 => get_l1_component_config(
                    l1_gas_price_provider.local(),
                    l1_events_provider.local(),
                    state_sync.remote(),
                    batcher.remote(),
                ),
                Self::Mempool => get_mempool_component_config(
                    mempool.local(),
                    class_manager.remote(),
                    gateway.remote(),
                    proof_manager.remote(),
                ),
                Self::ProofManager => get_proof_manager_component_config(proof_manager.local()),
                Self::SierraCompiler => {
                    get_sierra_compiler_component_config(sierra_compiler.local())
                }
                Self::SignatureManager => {
                    get_signature_manager_component_config(signature_manager.local())
                }
                Self::StateSync => {
                    get_state_sync_component_config(state_sync.local(), class_manager.remote())
                }
            };
            let node_service = inner_service_name.into();
            component_config_map.insert(node_service, component_config);
        }
        component_config_map
    }
}

>>>>>>> origin/main-v0.14.4
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
