use std::collections::{BTreeSet, HashSet};
use std::fmt::Display;

use serde::{Serialize, Serializer};
use strum::{Display, EnumDiscriminants, EnumIter, IntoEnumIterator, IntoStaticStr, VariantNames};

use crate::deployment_definitions::ComponentConfigInService;
use crate::deployments::consolidated::ConsolidatedNodeServiceName;
use crate::deployments::distributed::DistributedNodeServiceName;
use crate::deployments::hybrid::HybridNodeServiceName;

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
