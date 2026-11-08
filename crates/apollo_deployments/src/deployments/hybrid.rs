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

<<<<<<< HEAD
||||||| 01e4b43fa6
impl GetComponentConfigs for HybridNodeServiceName {
    fn get_component_configs(ports: Option<Vec<u16>>) -> HashMap<NodeService, ComponentConfig> {
        let mut infra_port_allocator =
            InfraPortAllocator::new(ports, HYBRID_NODE_REQUIRED_PORTS_NUM);
        let batcher = Self::Core.component_config_pair(infra_port_allocator.next());
        let class_manager = Self::Core.component_config_pair(infra_port_allocator.next());
        let committer = Self::Committer.component_config_pair(infra_port_allocator.next());
        let gateway = Self::Gateway.component_config_pair(infra_port_allocator.next());
        let l1_gas_price_provider = Self::L1.component_config_pair(infra_port_allocator.next());
        let l1_events_provider = Self::L1.component_config_pair(infra_port_allocator.next());
        let mempool = Self::Mempool.component_config_pair(infra_port_allocator.next());
        let proof_manager = Self::Core.component_config_pair(infra_port_allocator.next());
        let sierra_compiler =
            Self::SierraCompiler.component_config_pair(infra_port_allocator.next());
        let signature_manager = Self::Core.component_config_pair(infra_port_allocator.next());
        let state_sync = Self::Core.component_config_pair(infra_port_allocator.next());

        let mut component_config_map = HashMap::<NodeService, ComponentConfig>::new();
        for inner_service_name in Self::iter() {
            let component_config = match inner_service_name {
                Self::Committer => {
                    get_committer_component_config(committer.local(), batcher.remote())
                }
                Self::Core => get_core_component_config(
                    batcher.local(),
                    class_manager.local(),
                    committer.remote(),
                    l1_gas_price_provider.remote(),
                    l1_events_provider.remote(),
                    state_sync.local(),
                    mempool.remote(),
                    proof_manager.local(),
                    sierra_compiler.remote(),
                    signature_manager.local(),
                ),
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
                    batcher.remote(),
                    state_sync.remote(),
                ),
                Self::Mempool => get_mempool_component_config(
                    mempool.local(),
                    class_manager.remote(),
                    gateway.remote(),
                    proof_manager.remote(),
                ),
                Self::SierraCompiler => {
                    get_sierra_compiler_component_config(sierra_compiler.local())
                }
            };
            let node_service = inner_service_name.into();
            component_config_map.insert(node_service, component_config);
        }
        component_config_map
    }
}

=======
impl GetComponentConfigs for HybridNodeServiceName {
    fn get_component_configs(ports: Option<Vec<u16>>) -> HashMap<NodeService, ComponentConfig> {
        let mut infra_port_allocator =
            InfraPortAllocator::new(ports, HYBRID_NODE_REQUIRED_PORTS_NUM);
        let batcher = Self::Core.component_config_pair(infra_port_allocator.next());
        let class_manager = Self::Core.component_config_pair(infra_port_allocator.next());
        let committer =
            Self::Committer.component_config_pair(infra_port_allocator.next()).with_max_body_bytes(
                COMMITTER_MAX_REQUEST_BODY_BYTES,
                COMMITTER_MAX_RESPONSE_BODY_BYTES,
            );
        let gateway = Self::Gateway.component_config_pair(infra_port_allocator.next());
        let l1_gas_price_provider = Self::L1.component_config_pair(infra_port_allocator.next());
        let l1_events_provider = Self::L1.component_config_pair(infra_port_allocator.next());
        let mempool = Self::Mempool.component_config_pair(infra_port_allocator.next());
        let proof_manager = Self::Core.component_config_pair(infra_port_allocator.next());
        let sierra_compiler =
            Self::SierraCompiler.component_config_pair(infra_port_allocator.next());
        let signature_manager = Self::Core.component_config_pair(infra_port_allocator.next());
        let state_sync = Self::Core.component_config_pair(infra_port_allocator.next());

        let mut component_config_map = HashMap::<NodeService, ComponentConfig>::new();
        for inner_service_name in Self::iter() {
            let component_config = match inner_service_name {
                Self::Committer => {
                    get_committer_component_config(committer.local(), batcher.remote())
                }
                Self::Core => get_core_component_config(
                    batcher.local(),
                    class_manager.local(),
                    committer.remote(),
                    l1_gas_price_provider.remote(),
                    l1_events_provider.remote(),
                    state_sync.local(),
                    mempool.remote(),
                    proof_manager.local(),
                    sierra_compiler.remote(),
                    signature_manager.local(),
                ),
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
                    batcher.remote(),
                    state_sync.remote(),
                ),
                Self::Mempool => get_mempool_component_config(
                    mempool.local(),
                    class_manager.remote(),
                    gateway.remote(),
                    proof_manager.remote(),
                ),
                Self::SierraCompiler => {
                    get_sierra_compiler_component_config(sierra_compiler.local())
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
