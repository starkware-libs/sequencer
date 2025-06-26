use std::path::PathBuf;

use starknet_api::block::BlockNumber;

use crate::config_override::{
    ConfigOverride,
    DeploymentConfigOverride,
    InstanceConfigOverride,
    NetworkConfigOverride,
};
use crate::deployment::{Deployment, PragmaDomain};
use crate::deployment_definitions::{Environment, BASE_APP_CONFIG_PATH};
use crate::k8s::IngressParams;
use crate::service::NodeType;

const TESTING_INGRESS_DOMAIN: &str = "sw-dev.io";

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
        "https://integration-sepolia.starknet.io/",
        "0x1002",
        PragmaDomain::Dev,
        Some(BlockNumber(1)),
    )
}

fn testing_instance_config_override() -> InstanceConfigOverride {
    const SECRET_KEY: &str = "0x0101010101010101010101010101010101010101010101010101010101010101";

    InstanceConfigOverride::new(
        NetworkConfigOverride::new(None, None, SECRET_KEY),
        NetworkConfigOverride::new(None, None, SECRET_KEY),
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
        NodeType::DistributedNode,
        Environment::Testing,
        "deployment_test_distributed",
        None,
        PathBuf::from(BASE_APP_CONFIG_PATH),
        testing_config_override(),
        get_ingress_params(),
        None,
    )
}

fn system_test_hybrid_deployment() -> Deployment {
    Deployment::new(
        NodeType::HybridNode,
        Environment::Testing,
        "deployment_test_hybrid",
        None,
        PathBuf::from(BASE_APP_CONFIG_PATH),
        testing_config_override(),
        get_ingress_params(),
        None,
    )
}

fn system_test_consolidated_deployment() -> Deployment {
    Deployment::new(
        NodeType::ConsolidatedNode,
        Environment::Testing,
        "deployment_test_consolidated",
        None,
        PathBuf::from(BASE_APP_CONFIG_PATH),
        testing_config_override(),
        get_ingress_params(),
        None,
    )
}
