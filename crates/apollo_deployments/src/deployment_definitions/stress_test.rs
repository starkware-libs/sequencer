use std::path::PathBuf;

use starknet_api::core::ChainId;

use crate::deployment::{
    create_hybrid_instance_config_override,
    ConfigOverride,
    Deployment,
    DeploymentConfigOverride,
};
use crate::deployment_definitions::{Environment, BASE_APP_CONFIG_PATH};
use crate::service::{DeploymentName, ExternalSecret, IngressParams};

const STRESS_TEST_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME: &str = "apollo-stresstest-dev.sw-dev.io";
const STRESS_TEST_INGRESS_DOMAIN: &str = "sw-dev.io";
const FIRST_NODE_NAMESPACE: &str = "apollo-stresstest-dev-0";

pub(crate) fn stress_test_hybrid_deployments() -> Vec<Deployment> {
    vec![
        stress_test_hybrid_deployment_node_0(),
        stress_test_hybrid_deployment_node_1(),
        stress_test_hybrid_deployment_node_2(),
        stress_test_hybrid_deployment_node_3(),
    ]
}

fn stress_test_deployment_config_override() -> DeploymentConfigOverride {
    DeploymentConfigOverride::new(
        "0x4fA369fEBf0C574ea05EC12bC0e1Bc9Cd461Dd0f",
        "SN_GOERLI",
        "0x497d1c054cec40f64454b45deecdc83e0c7f7b961c63531eae03748abd95350",
        "http://feeder-gateway.starknet-0-14-0-stress-test/",
        "0x4fa9355c504fa2de263bd7920644b5e48794fe1450ec2a6526518ad77d6a567",
    )
}

fn stress_test_node_0_config_override() -> ConfigOverride {
    ConfigOverride::new(
        stress_test_deployment_config_override(),
        create_hybrid_instance_config_override(0, FIRST_NODE_NAMESPACE),
    )
}
fn stress_test_node_1_config_override() -> ConfigOverride {
    ConfigOverride::new(
        stress_test_deployment_config_override(),
        create_hybrid_instance_config_override(1, FIRST_NODE_NAMESPACE),
    )
}
fn stress_test_node_2_config_override() -> ConfigOverride {
    ConfigOverride::new(
        stress_test_deployment_config_override(),
        create_hybrid_instance_config_override(2, FIRST_NODE_NAMESPACE),
    )
}
fn stress_test_node_3_config_override() -> ConfigOverride {
    ConfigOverride::new(
        stress_test_deployment_config_override(),
        create_hybrid_instance_config_override(3, FIRST_NODE_NAMESPACE),
    )
}

fn get_ingress_params() -> IngressParams {
    IngressParams::new(
        STRESS_TEST_INGRESS_DOMAIN.to_string(),
        Some(vec![STRESS_TEST_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
    )
}

fn stress_test_hybrid_deployment_node_0() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::StressTest,
        "integration_hybrid_node_0",
        Some(ExternalSecret::new("apollo-stresstest-dev-0")),
        PathBuf::from(BASE_APP_CONFIG_PATH),
        stress_test_node_0_config_override(),
        get_ingress_params(),
    )
}

fn stress_test_hybrid_deployment_node_1() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::StressTest,
        "integration_hybrid_node_1",
        Some(ExternalSecret::new("apollo-stresstest-dev-1")),
        PathBuf::from(BASE_APP_CONFIG_PATH),
        stress_test_node_1_config_override(),
        get_ingress_params(),
    )
}

fn stress_test_hybrid_deployment_node_2() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::StressTest,
        "integration_hybrid_node_2",
        Some(ExternalSecret::new("apollo-stresstest-dev-2")),
        PathBuf::from(BASE_APP_CONFIG_PATH),
        stress_test_node_2_config_override(),
        get_ingress_params(),
    )
}

fn stress_test_hybrid_deployment_node_3() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::StressTest,
        "integration_hybrid_node_3",
        Some(ExternalSecret::new("apollo-stresstest-dev-3")),
        PathBuf::from(BASE_APP_CONFIG_PATH),
        stress_test_node_3_config_override(),
        get_ingress_params(),
    )
}
