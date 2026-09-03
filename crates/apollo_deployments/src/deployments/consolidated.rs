use std::collections::BTreeSet;

use serde::Serialize;
use strum::{AsRefStr, Display, EnumIter, EnumString, IntoEnumIterator};

use crate::deployment_definitions::ComponentConfigInService;
use crate::service::{NodeService, ServiceNameInner};

#[derive(
    Clone, Copy, Debug, Display, EnumString, PartialEq, Eq, Hash, Serialize, AsRefStr, EnumIter,
)]
#[strum(serialize_all = "snake_case")]
pub enum ConsolidatedNodeServiceName {
    Node,
}

impl From<ConsolidatedNodeServiceName> for NodeService {
    fn from(service: ConsolidatedNodeServiceName) -> Self {
        NodeService::Consolidated(service)
    }
}

impl ServiceNameInner for ConsolidatedNodeServiceName {
    fn get_components_in_service(&self) -> BTreeSet<ComponentConfigInService> {
        match self {
            Self::Node => ComponentConfigInService::iter().collect(),
        }
    }
}
