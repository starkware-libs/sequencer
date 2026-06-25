use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Display;

use apollo_node_config::component_config::ComponentConfig;
use apollo_node_config::component_execution_config::ReactiveComponentExecutionConfig;
use serde::{Serialize, Serializer};
use strum::{Display, EnumDiscriminants, EnumIter, IntoEnumIterator, IntoStaticStr, VariantNames};

use crate::deployment_definitions::ComponentConfigInService;
use crate::deployments::consolidated::ConsolidatedNodeServiceName;
use crate::deployments::distributed::DistributedNodeServiceName;
use crate::deployments::hybrid::HybridNodeServiceName;
use crate::scale_policy::ScalePolicy;

const REMOTE_SERVICE_URL_PLACEHOLDER: &str = "remote_service";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumDiscriminants)]
#[strum_discriminants(
    name(NodeType),
    derive(IntoStaticStr, EnumIter, VariantNames, Serialize, Display),
    strum(serialize_all = "snake_case")
)]
pub enum NodeService {
    Consolidated(ConsolidatedNodeServiceName),
    Hybrid(HybridNodeServiceName),
    Distributed(DistributedNodeServiceName),
}

impl NodeService {
    fn as_inner(&self) -> &dyn ServiceNameInner {
        match self {
            NodeService::Consolidated(inner) => inner,
            NodeService::Hybrid(inner) => inner,
            NodeService::Distributed(inner) => inner,
        }
    }

    pub fn get_components_in_service(&self) -> BTreeSet<ComponentConfigInService> {
        self.as_inner().get_components_in_service()
    }
}

pub(crate) trait ServiceNameInner: Display {
    fn get_scale_policy(&self) -> ScalePolicy;

    fn get_retries(&self) -> usize;

    fn get_components_in_service(&self) -> BTreeSet<ComponentConfigInService>;
}

impl NodeType {
    pub fn all_service_names(&self) -> Vec<NodeService> {
        match self {
            // TODO(Tsabary): find a way to avoid this code duplication.
            Self::Consolidated => {
                ConsolidatedNodeServiceName::iter().map(NodeService::Consolidated).collect()
            }
            Self::Hybrid => HybridNodeServiceName::iter().map(NodeService::Hybrid).collect(),
            Self::Distributed => {
                DistributedNodeServiceName::iter().map(NodeService::Distributed).collect()
            }
        }
    }

    pub fn get_services_of_components(
        &self,
        component_type: ComponentConfigInService,
    ) -> HashSet<NodeService> {
        let services: HashSet<_> = self
            .all_service_names()
            .into_iter()
            .filter(|node_service| {
                node_service.get_components_in_service().contains(&component_type)
            })
            .collect();

        assert!(
            !services.is_empty(),
            "Expected at least one NodeService containing component type {:?}",
            component_type
        );

        services
    }

    pub fn get_component_configs(
        &self,
        ports: Option<Vec<u16>>,
    ) -> HashMap<NodeService, ComponentConfig> {
        match self {
            // TODO(Tsabary): avoid this code duplication.
            Self::Consolidated => ConsolidatedNodeServiceName::get_component_configs(ports),
            Self::Hybrid => HybridNodeServiceName::get_component_configs(ports),
            Self::Distributed => DistributedNodeServiceName::get_component_configs(ports),
        }
    }
}

pub(crate) trait GetComponentConfigs: ServiceNameInner {
    fn get_component_configs(ports: Option<Vec<u16>>) -> HashMap<NodeService, ComponentConfig>;

    /// Returns a component execution config for a component that runs locally, and accepts inbound
    /// connections from remote components.
    fn component_config_for_local_service(&self, port: u16) -> ReactiveComponentExecutionConfig {
        ReactiveComponentExecutionConfig::local_with_remote_enabled(
            REMOTE_SERVICE_URL_PLACEHOLDER.to_string(),
            port,
        )
    }

    /// Returns a component execution config for a component that is accessed remotely.
    fn component_config_for_remote_service(&self, port: u16) -> ReactiveComponentExecutionConfig {
        let idle_connections = self.get_scale_policy().idle_connections();
        let retries = self.get_retries();
        ReactiveComponentExecutionConfig::remote(REMOTE_SERVICE_URL_PLACEHOLDER.to_string(), port)
            .with_idle_connections(idle_connections)
            .with_retries(retries)
    }

    fn component_config_pair(&self, port: u16) -> ComponentConfigPair {
        ComponentConfigPair {
            local: self.component_config_for_local_service(port),
            remote: self.component_config_for_remote_service(port),
        }
    }
}

/// Component config bundling for node services: a config to run a component
/// locally while being accessible to other remote components, and a suitable remote-access config
/// to be used by such remotes.
pub(crate) struct ComponentConfigPair {
    local: ReactiveComponentExecutionConfig,
    remote: ReactiveComponentExecutionConfig,
}

impl ComponentConfigPair {
    pub(crate) fn local(&self) -> ReactiveComponentExecutionConfig {
        self.local.clone()
    }

    pub(crate) fn remote(&self) -> ReactiveComponentExecutionConfig {
        self.remote.clone()
    }
}

impl Serialize for NodeService {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Serialize only the inner value.
        match self {
            NodeService::Consolidated(inner) => inner.serialize(serializer),
            NodeService::Hybrid(inner) => inner.serialize(serializer),
            NodeService::Distributed(inner) => inner.serialize(serializer),
        }
    }
}
