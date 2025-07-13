use apollo_infra_utils::template::Template;
use url::Url;

use crate::config_override::{ConfigOverride, DeploymentConfigOverride};
use crate::deployment::{Deployment, P2PCommunicationType};
use crate::deployment_definitions::{CloudK8sEnvironment, Environment, StateSyncType};
use crate::deployments::hybrid::{create_hybrid_instance_config_override, INSTANCE_NAME_FORMAT};
use crate::k8s::{ExternalSecret, IngressParams, K8sServiceConfigParams};
use crate::service::NodeType;

const NODE_IDS: [usize; 3] = [0, 1, 2];
const HTTP_SERVER_INGRESS_ALTERNATIVE_NAME: &str = "placeholder.starknet.io";
const INGRESS_DOMAIN: &str = "starknet.io";
const SECRET_NAME_FORMAT: Template = Template("placeholder-{}");
const NODE_NAMESPACE_FORMAT: Template = Template("placeholder-{}");

pub(crate) fn mainnet_hybrid_deployments() -> Vec<Deployment> {
    NODE_IDS.map(|i| hybrid_deployments(i, P2PCommunicationType::External)).to_vec()
}

// TODO(Tsabary): currently set with testnet values, need to be updated.

// TODO(Tsabary): for all envs, define the values as constants at the top of the module, and cancel
// the inner function calls.
fn deployment_config_override() -> DeploymentConfigOverride {
    DeploymentConfigOverride::new(
        "0xdeadbeef",
        "SN_PLACEHOLDER",
        "0xdeadbeef",
        Url::parse("https://feeder.placeholder.starknet.io").expect("Invalid URL"),
        "0xdeadbeef",
        None,
        NODE_IDS.len(),
        StateSyncType::Central,
    )
}

fn hybrid_deployments(id: usize, p2p_communication_type: P2PCommunicationType) -> Deployment {
    Deployment::new(
        NodeType::Hybrid,
        Environment::CloudK8s(CloudK8sEnvironment::Mainnet),
        &INSTANCE_NAME_FORMAT.format(&[&id]),
        Some(ExternalSecret::new(SECRET_NAME_FORMAT.format(&[&id]))),
        ConfigOverride::new(
            deployment_config_override(),
            create_hybrid_instance_config_override(
                id,
                NODE_NAMESPACE_FORMAT,
                p2p_communication_type,
                INGRESS_DOMAIN,
            ),
        ),
        IngressParams::new(
            INGRESS_DOMAIN.to_string(),
            Some(vec![HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
        ),
        Some(K8sServiceConfigParams::new(
            NODE_NAMESPACE_FORMAT.format(&[&id]),
            INGRESS_DOMAIN.to_string(),
            P2PCommunicationType::External,
        )),
    )
}
