use serde::{Serialize, Serializer};

use crate::deployment::P2PCommunicationType;
use crate::deployment_definitions::Environment;

// Controls whether external P2P communication is enabled.
const INTERNAL_ONLY_P2P_COMMUNICATION: bool = true;

const INGRESS_ROUTE: &str = "/gateway";
const INGRESS_PORT: u16 = 8080;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum Controller {
    Deployment,
    StatefulSet,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum K8SServiceType {
    // TODO(Tsabary): remove dead_code annotations when instances require these variants.
    #[allow(dead_code)]
    ClusterIp,
    LoadBalancer,
    #[allow(dead_code)]
    NodePort,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct K8sServiceConfig {
    #[serde(rename = "type")]
    k8s_service_type: K8SServiceType,
    external_dns_name: Option<String>,
    internal: bool,
}

impl K8sServiceConfig {
    pub fn new(
        external_dns_name: Option<String>,
        p2p_communication_type: P2PCommunicationType,
    ) -> Self {
        Self {
            k8s_service_type: p2p_communication_type.get_k8s_service_type(),
            external_dns_name,
            internal: INTERNAL_ONLY_P2P_COMMUNICATION,
        }
    }
}

#[derive(Clone)]
pub struct K8sServiceConfigParams {
    pub namespace: String,
    pub domain: String,
    pub p2p_communication_type: P2PCommunicationType,
}

impl K8sServiceConfigParams {
    pub fn new(
        namespace: String,
        domain: String,
        p2p_communication_type: P2PCommunicationType,
    ) -> Self {
        Self { namespace, domain, p2p_communication_type }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Ingress {
    #[serde(flatten)]
    ingress_params: IngressParams,
    internal: bool,
    rules: Vec<IngressRule>,
}

impl Ingress {
    pub fn new(ingress_params: IngressParams, internal: bool, rules: Vec<IngressRule>) -> Self {
        Self { ingress_params, internal, rules }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct IngressParams {
    domain: String,
    #[serde(serialize_with = "serialize_none_as_empty_vec")]
    alternative_names: Option<Vec<String>>,
}

fn serialize_none_as_empty_vec<S, T>(
    value: &Option<Vec<T>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: Serialize,
{
    match value {
        Some(v) => serializer.serialize_some(v),
        None => serializer.serialize_some(&Vec::<T>::new()),
    }
}

impl IngressParams {
    pub fn new(domain: String, alternative_names: Option<Vec<String>>) -> Self {
        Self { domain, alternative_names }
    }
}

pub(crate) fn get_ingress(ingress_params: IngressParams, internal: bool) -> Option<Ingress> {
    Some(Ingress::new(
        ingress_params,
        internal,
        vec![IngressRule::new(String::from(INGRESS_ROUTE), INGRESS_PORT, None)],
    ))
}

pub(crate) fn get_environment_ingress_internal(environment: &Environment) -> bool {
    match environment {
        Environment::CloudK8s(_) => false,
        Environment::LocalK8s => true,
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct IngressRule {
    path: String,
    port: u16,
    backend: Option<String>,
}

impl IngressRule {
    pub fn new(path: String, port: u16, backend: Option<String>) -> Self {
        Self { path, port, backend }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ExternalSecret {
    gcsm_key: String,
}

impl ExternalSecret {
    pub fn new(gcsm_key: impl ToString) -> Self {
        Self { gcsm_key: gcsm_key.to_string() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Resource {
    cpu: usize,
    memory: usize,
}

impl Resource {
    pub fn new(cpu: usize, memory: usize) -> Self {
        Self { cpu, memory }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Resources {
    requests: Resource,
    limits: Resource,
}

impl Resources {
    pub fn new(requests: Resource, limits: Resource) -> Self {
        Self { requests, limits }
    }
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Toleration {
    ApolloCoreService,
    #[serde(rename = "apollo-core-service-c2d-16")]
    ApolloCoreServiceC2D16,
    #[serde(rename = "apollo-core-service-c2d-32")]
    ApolloCoreServiceC2D32,
    #[serde(rename = "apollo-core-service-c2d-56")]
    ApolloCoreServiceC2D56,
    ApolloGeneralService,
    #[serde(rename = "batcher-8-64")]
    Batcher864,
}
