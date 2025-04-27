use std::path::PathBuf;

use apollo_infra::component_definitions::{LocalServerConfig, RemoteClientConfig};
use starknet_api::core::ChainId;
use strum_macros::{Display, EnumString};

use crate::deployment::{
    ConfigOverride,
    Deployment,
    DeploymentConfigOverride,
    InstanceConfigOverride,
};
use crate::service::{DeploymentName, ExternalSecret};

#[cfg(test)]
#[path = "deployment_definitions_test.rs"]
mod deployment_definitions_test;

// TODO(Tsabary): separate deployments to different modules.

const SYSTEM_TEST_BASE_APP_CONFIG_PATH: &str =
    "config/sequencer/testing/base_app_configs/single_node_deployment_test.json";

const INTEGRATION_BASE_APP_CONFIG_PATH: &str =
    "config/sequencer/sepolia_integration/base_app_configs/node.json";

pub(crate) const CONFIG_BASE_DIR: &str = "config/sequencer/";
const APP_CONFIGS_DIR_NAME: &str = "app_configs/";

const SEPOLIA_INTEGRATION_DEPLOYMENT_CONFIG_OVERRIDE: DeploymentConfigOverride =
    DeploymentConfigOverride::new(
        "0xa23a6BA7DA61988D2420dAE9F10eE964552459d5",
        "SN_GOERLI",
        "0x7c07a3eec8ff611328722c3fc3e5d2e4ef2f60740c0bf86c756606036b74c16",
        "https://fgw-sn-test-sepolia-3-sepolia.gateway-proxy.sw-dev.io",
        "0x54a93d918d62b2fb62b25e77d9cb693bd277ab7e6fa236e53af263f1adb40e4",
    );

const TESTING_DEPLOYMENT_CONFIG_OVERRIDE: DeploymentConfigOverride = DeploymentConfigOverride::new(
    "0x5FbDB2315678afecb367f032d93F642f64180aa3",
    "CHAIN_ID_SUBDIR",
    "0x1001",
    "https://integration-sepolia.starknet.io/",
    "0x1002",
);

const SEPOLIA_INTEGRATION_NODE_0_INSTANCE_CONFIG_OVERRIDE: InstanceConfigOverride =
    InstanceConfigOverride::new(
        "",
        true,
        "0x0101010101010101010101010101010101010101010101010101010101010101",
        "",
        true,
        "0x0101010101010101010101010101010101010101010101010101010101010101",
        "0x1",
    );

const SEPOLIA_INTEGRATION_NODE_1_INSTANCE_CONFIG_OVERRIDE: InstanceConfigOverride =
    InstanceConfigOverride::new(
        "/dns/sequencer-core-service.sequencer-test-3-node-0.svc.cluster.local/tcp/53080/p2p/\
         12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        false,
        "0x0101010101010101010101010101010101010101010101010101010101010102",
        "/dns/sequencer-mempool-service.sequencer-test-3-node-0.svc.cluster.local/tcp/53200/p2p/\
         12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        false,
        "0x0101010101010101010101010101010101010101010101010101010101010102",
        "0x2",
    );

const SEPOLIA_INTEGRATION_NODE_2_INSTANCE_CONFIG_OVERRIDE: InstanceConfigOverride =
    InstanceConfigOverride::new(
        "/dns/sequencer-core-service.sequencer-test-3-node-0.svc.cluster.local/tcp/53080/p2p/\
         12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        false,
        "0x0101010101010101010101010101010101010101010101010101010101010103",
        "/dns/sequencer-mempool-service.sequencer-test-3-node-0.svc.cluster.local/tcp/53200/p2p/\
         12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        false,
        "0x0101010101010101010101010101010101010101010101010101010101010103",
        "0x3",
    );

const SEPOLIA_INTEGRATION_NODE_3_INSTANCE_CONFIG_OVERRIDE: InstanceConfigOverride =
    InstanceConfigOverride::new(
        "/dns/sequencer-core-service.sequencer-test-3-node-0.svc.cluster.local/tcp/53080/p2p/\
         12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        false,
        "0x0101010101010101010101010101010101010101010101010101010101010104",
        "/dns/sequencer-mempool-service.sequencer-test-3-node-0.svc.cluster.local/tcp/53200/p2p/\
         12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        false,
        "0x0101010101010101010101010101010101010101010101010101010101010104",
        "0x4",
    );

const TESTING_INSTANCE_CONFIG_OVERRIDE: InstanceConfigOverride = InstanceConfigOverride::new(
    "",
    true,
    "0x0101010101010101010101010101010101010101010101010101010101010101",
    "",
    true,
    "0x0101010101010101010101010101010101010101010101010101010101010101",
    "0x64",
);

const SEPOLIA_INTEGRATION_NODE_0_CONFIG_OVERRIDE: ConfigOverride = ConfigOverride::new(
    &SEPOLIA_INTEGRATION_DEPLOYMENT_CONFIG_OVERRIDE,
    &SEPOLIA_INTEGRATION_NODE_0_INSTANCE_CONFIG_OVERRIDE,
);
const SEPOLIA_INTEGRATION_NODE_1_CONFIG_OVERRIDE: ConfigOverride = ConfigOverride::new(
    &SEPOLIA_INTEGRATION_DEPLOYMENT_CONFIG_OVERRIDE,
    &SEPOLIA_INTEGRATION_NODE_1_INSTANCE_CONFIG_OVERRIDE,
);
const SEPOLIA_INTEGRATION_NODE_2_CONFIG_OVERRIDE: ConfigOverride = ConfigOverride::new(
    &SEPOLIA_INTEGRATION_DEPLOYMENT_CONFIG_OVERRIDE,
    &SEPOLIA_INTEGRATION_NODE_2_INSTANCE_CONFIG_OVERRIDE,
);
const SEPOLIA_INTEGRATION_NODE_3_CONFIG_OVERRIDE: ConfigOverride = ConfigOverride::new(
    &SEPOLIA_INTEGRATION_DEPLOYMENT_CONFIG_OVERRIDE,
    &SEPOLIA_INTEGRATION_NODE_3_INSTANCE_CONFIG_OVERRIDE,
);
const TESTING_CONFIG_OVERRIDE: ConfigOverride =
    ConfigOverride::new(&TESTING_DEPLOYMENT_CONFIG_OVERRIDE, &TESTING_INSTANCE_CONFIG_OVERRIDE);

const SEPOLIA_INTEGRATION_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME: &str =
    "sn-test-sepolia-3-sepolia.gateway-proxy.sw-dev.io";

const SEPOLIA_INTEGRATION_INGRESS_DOMAIN: &str = "sw-dev.io";
const TESTING_INGRESS_DOMAIN: &str = "sw-dev.io";

type DeploymentFn = fn() -> Deployment;

// TODO(Tsabary): create deployment instances per per deployment.

pub const DEPLOYMENTS: &[DeploymentFn] = &[
    system_test_distributed_deployment,
    system_test_consolidated_deployment,
    sepolia_integration_hybrid_deployment_node_0,
    sepolia_integration_hybrid_deployment_node_1,
    sepolia_integration_hybrid_deployment_node_2,
    sepolia_integration_hybrid_deployment_node_3,
];

// Integration deployments

fn sepolia_integration_hybrid_deployment_node_0() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::SepoliaIntegration,
        "integration_hybrid_node_0",
        Some(ExternalSecret::new("sequencer-test-3-node-0")),
        PathBuf::from(INTEGRATION_BASE_APP_CONFIG_PATH),
        SEPOLIA_INTEGRATION_NODE_0_CONFIG_OVERRIDE,
        SEPOLIA_INTEGRATION_INGRESS_DOMAIN.to_string(),
        Some(vec![SEPOLIA_INTEGRATION_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
    )
}

fn sepolia_integration_hybrid_deployment_node_1() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::SepoliaIntegration,
        "integration_hybrid_node_1",
        Some(ExternalSecret::new("sequencer-test-3-node-1")),
        PathBuf::from(INTEGRATION_BASE_APP_CONFIG_PATH),
        SEPOLIA_INTEGRATION_NODE_1_CONFIG_OVERRIDE,
        SEPOLIA_INTEGRATION_INGRESS_DOMAIN.to_string(),
        Some(vec![SEPOLIA_INTEGRATION_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
    )
}

fn sepolia_integration_hybrid_deployment_node_2() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::SepoliaIntegration,
        "integration_hybrid_node_2",
        Some(ExternalSecret::new("sequencer-test-3-node-2")),
        PathBuf::from(INTEGRATION_BASE_APP_CONFIG_PATH),
        SEPOLIA_INTEGRATION_NODE_2_CONFIG_OVERRIDE,
        SEPOLIA_INTEGRATION_INGRESS_DOMAIN.to_string(),
        Some(vec![SEPOLIA_INTEGRATION_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
    )
}

fn sepolia_integration_hybrid_deployment_node_3() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::SepoliaIntegration,
        "integration_hybrid_node_3",
        Some(ExternalSecret::new("sequencer-test-3-node-3")),
        PathBuf::from(INTEGRATION_BASE_APP_CONFIG_PATH),
        SEPOLIA_INTEGRATION_NODE_3_CONFIG_OVERRIDE,
        SEPOLIA_INTEGRATION_INGRESS_DOMAIN.to_string(),
        Some(vec![SEPOLIA_INTEGRATION_HTTP_SERVER_INGRESS_ALTERNATIVE_NAME.into()]),
    )
}

// System test deployments
fn system_test_distributed_deployment() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::DistributedNode,
        Environment::Testing,
        "deployment_test_distributed",
        None,
        PathBuf::from(SYSTEM_TEST_BASE_APP_CONFIG_PATH),
        TESTING_CONFIG_OVERRIDE,
        TESTING_INGRESS_DOMAIN.to_string(),
        None,
    )
}

fn system_test_consolidated_deployment() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::ConsolidatedNode,
        Environment::Testing,
        "deployment_test_consolidated",
        None,
        PathBuf::from(SYSTEM_TEST_BASE_APP_CONFIG_PATH),
        TESTING_CONFIG_OVERRIDE,
        TESTING_INGRESS_DOMAIN.to_string(),
        None,
    )
}

#[derive(EnumString, Clone, Display, PartialEq, Debug)]
#[strum(serialize_all = "snake_case")]
pub enum Environment {
    Testing,
    SepoliaIntegration,
    SepoliaTestnet,
    Mainnet,
}

impl Environment {
    pub fn application_config_dir_path(&self) -> PathBuf {
        PathBuf::from(CONFIG_BASE_DIR).join(self.to_string()).join(APP_CONFIGS_DIR_NAME)
    }

    pub fn get_component_config_modifications(&self) -> EnvironmentComponentConfigModifications {
        match self {
            Environment::Testing => EnvironmentComponentConfigModifications::testing(),
            Environment::SepoliaIntegration => {
                EnvironmentComponentConfigModifications::sepolia_integration()
            }
            Environment::SepoliaTestnet => unimplemented!("SepoliaTestnet is not implemented yet"),
            Environment::Mainnet => unimplemented!("Mainnet is not implemented yet"),
        }
    }
}

pub struct EnvironmentComponentConfigModifications {
    pub local_server_config: LocalServerConfig,
    pub max_concurrency: usize,
    pub remote_client_config: RemoteClientConfig,
}

impl EnvironmentComponentConfigModifications {
    pub fn testing() -> Self {
        Self {
            local_server_config: LocalServerConfig { channel_buffer_size: 32 },
            max_concurrency: 10,
            remote_client_config: RemoteClientConfig {
                retries: 3,
                idle_connections: 5,
                idle_timeout: 90,
                retry_interval: 3,
            },
        }
    }

    pub fn sepolia_integration() -> Self {
        Self {
            local_server_config: LocalServerConfig { channel_buffer_size: 128 },
            max_concurrency: 100,
            remote_client_config: RemoteClientConfig {
                retries: 3,
                idle_connections: usize::MAX,
                idle_timeout: 1,
                retry_interval: 1,
            },
        }
    }
}
