use apollo_infra_utils::template::Template;

use crate::config_override::{ConfigOverride, DeploymentConfigOverride};
use crate::deployment::{Deployment, P2PCommunicationType, PragmaDomain};
use crate::deployment_definitions::Environment;
use crate::deployments::hybrid::{create_hybrid_instance_config_override, INSTANCE_NAME_FORMAT};
use crate::k8s::{ExternalSecret, IngressParams};
use crate::service::NodeType;

const SEPOLIA_INTEGRATION_NODE_IDS: [usize; 3] = [0, 1, 2];
const SEPOLIA_INTEGRATION_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME: &str =
    "integration-sepolia.starknet.io";
const SEPOLIA_INTEGRATION_INGRESS_DOMAIN: &str = "starknet.io";
const SECRET_NAME_FORMAT: Template = Template("apollo-sepolia-integration-{}");
const NODE_NAMESPACE_FORMAT: Template = Template("apollo-sepolia-integration-{}");

pub(crate) fn sepolia_integration_hybrid_deployments() -> Vec<Deployment> {
    SEPOLIA_INTEGRATION_NODE_IDS
        .map(|i| sepolia_integration_hybrid_deployment_node(i, P2PCommunicationType::Internal))
        .to_vec()
}

fn sepolia_integration_deployment_config_override() -> DeploymentConfigOverride {
    DeploymentConfigOverride::new(
        "0x4737c0c1B4D5b1A687B42610DdabEE781152359c",
        "SN_INTEGRATION_SEPOLIA",
        "0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7",
        "https://feeder.integration-sepolia.starknet.io/",
        "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d",
        PragmaDomain::Dev,
        None,
        SEPOLIA_INTEGRATION_NODE_IDS.len(),
    )
}

fn sepolia_integration_hybrid_deployment_node(
    id: usize,
    p2p_communication_type: P2PCommunicationType,
) -> Deployment {
    Deployment::new(
        NodeType::Hybrid,
        Environment::SepoliaIntegration,
        &INSTANCE_NAME_FORMAT.format(&[&id]),
        Some(ExternalSecret::new(SECRET_NAME_FORMAT.format(&[&id]))),
        ConfigOverride::new(
            sepolia_integration_deployment_config_override(),
            create_hybrid_instance_config_override(
                id,
                NODE_NAMESPACE_FORMAT,
                p2p_communication_type,
                SEPOLIA_INTEGRATION_INGRESS_DOMAIN,
            ),
        ),
        IngressParams::new(
            SEPOLIA_INTEGRATION_INGRESS_DOMAIN.to_string(),
            Some(vec![SEPOLIA_INTEGRATION_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
        ),
        None,
    )
}
