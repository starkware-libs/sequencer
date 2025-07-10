use starknet_api::block::BlockNumber;
use url::Url;

use crate::config_override::{
    ConfigOverride,
    DeploymentConfigOverride,
    InstanceConfigOverride,
    NetworkConfigOverride,
};
use crate::deployment::Deployment;
use crate::deployment_definitions::{Environment, StateSyncType};
use crate::k8s::IngressParams;
use crate::service::NodeType;

const TESTING_INGRESS_DOMAIN: &str = "sw-dev.io";
const TESTING_NODE_IDS: [usize; 1] = [0];

pub(crate) fn system_test_deployments() -> Vec<Deployment> {
    vec![
        system_test_distributed_deployment(),
        system_test_hybrid_deployment(),
        system_test_consolidated_deployment(),
    ]
}

fn testing_deployment_config_override() -> DeploymentConfigOverride {
    DeploymentConfigOverride::new(
        "0x5FbDB2315678afecb367f032d93F642f64180aa3",
        "CHAIN_ID_SUBDIR",
        "0x1001",
        Url::parse("https://integration-sepolia.starknet.io/").expect("Invalid URL"),
        "0x1002",
        Some(BlockNumber(1)),
        TESTING_NODE_IDS.len(),
        StateSyncType::P2P,
    )
}

fn testing_instance_config_override() -> InstanceConfigOverride {
    InstanceConfigOverride::new(
        NetworkConfigOverride::new(None, None),
        NetworkConfigOverride::new(None, None),
        "0x64",
    )
}

fn testing_config_override() -> ConfigOverride {
    ConfigOverride::new(testing_deployment_config_override(), testing_instance_config_override())
}

fn get_ingress_params() -> IngressParams {
    IngressParams::new(TESTING_INGRESS_DOMAIN.to_string(), None)
}

fn system_test_distributed_deployment() -> Deployment {
    Deployment::new(
        NodeType::Distributed,
        Environment::Testing,
        "distributed",
        None,
        testing_config_override(),
        get_ingress_params(),
        None,
    )
}

fn system_test_hybrid_deployment() -> Deployment {
    Deployment::new(
        NodeType::Hybrid,
        Environment::Testing,
        "hybrid",
        None,
        testing_config_override(),
        get_ingress_params(),
        None,
    )
}

fn system_test_consolidated_deployment() -> Deployment {
    Deployment::new(
        NodeType::Consolidated,
        Environment::Testing,
        "consolidated",
        None,
        testing_config_override(),
        get_ingress_params(),
        None,
    )
}
