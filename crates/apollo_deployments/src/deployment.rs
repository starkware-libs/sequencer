use serde::{Deserialize, Serialize};

use crate::k8s::K8SServiceType;

// TODO(Tsabary): consider unifying pointer targets to a single file.

// Creates the service name in the format: <node_service>.<namespace>.<domain>
pub(crate) fn build_service_namespace_domain_address(
    node_service: &str,
    namespace: &str,
    domain: &str,
) -> String {
    format!("{node_service}.{namespace}.{domain}")
}

// TODO(Tsabary): when transitioning runnings nodes in different clusters, this enum should be
// removed, and the p2p address should always be `External`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum P2PCommunicationType {
    Internal,
    External,
}

impl P2PCommunicationType {
    pub(crate) fn get_k8s_service_type(&self) -> K8SServiceType {
        K8SServiceType::LoadBalancer
    }
}
