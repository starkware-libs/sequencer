use std::path::PathBuf;

use apollo_infra::component_client::RemoteClientConfig;
use apollo_infra::component_server::LocalServerConfig;
use starknet_api::core::ChainId;
use strum_macros::{Display, EnumString};

use crate::deployment::{
    ConfigOverride,
    Deployment,
    DeploymentConfigOverride,
    InstanceConfigOverride,
    DEPLOYMENT_IMAGE_FOR_PRE_INTEGRATION,
    DEPLOYMENT_IMAGE_FOR_TESTING,
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

const INTEGRATION_DEPLOYMENT_CONFIG_OVERRIDE: DeploymentConfigOverride =
    DeploymentConfigOverride::new(
        "0xA43812F9C610851daF67c5FA36606Ea8c8Fa7caE",
        "SEPOLIA_INTEGRATION",
        "0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7",
        "https://feeder.integration-sepolia.starknet.io/",
        "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d",
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
        "true",
        "0x0101010101010101010101010101010101010101010101010101010101010101",
        "",
        "true",
        "0x0101010101010101010101010101010101010101010101010101010101010101",
        "0x1",
    );

// TODO(Tsabary): need to properly edit the peer addresses using the correct cluster, namespace, and
// port values.
const SEPOLIA_INTEGRATION_NODE_1_INSTANCE_CONFIG_OVERRIDE: InstanceConfigOverride =
    InstanceConfigOverride::new(
        "/dns/sequencer-core-service.sequencer0-preintegration.svc.cluster.local/tcp/53080/p2p/\
         12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        "false",
        "0x0101010101010101010101010101010101010101010101010101010101010102",
        "/dns/sequencer-mempool-service.sequencer0-preintegration.svc.cluster.local/tcp/53200/p2p/\
         12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        "false",
        "0x0101010101010101010101010101010101010101010101010101010101010102",
        "0x2",
    );

// TODO(Tsabary): need to properly edit the peer addresses using the correct cluster, namespace, and
// port values.
const SEPOLIA_INTEGRATION_NODE_2_INSTANCE_CONFIG_OVERRIDE: InstanceConfigOverride =
    InstanceConfigOverride::new(
        "/dns/sequencer-core-service.sequencer0-preintegration.svc.cluster.local/tcp/53080/p2p/\
         12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        "false",
        "0x0101010101010101010101010101010101010101010101010101010101010103",
        "/dns/sequencer-mempool-service.sequencer0-preintegration.svc.cluster.local/tcp/53200/p2p/\
         12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        "false",
        "0x0101010101010101010101010101010101010101010101010101010101010103",
        "0x3",
    );

// TODO(Tsabary): need to properly edit the peer addresses using the correct cluster, namespace, and
// port values.
const SEPOLIA_INTEGRATION_NODE_3_INSTANCE_CONFIG_OVERRIDE: InstanceConfigOverride =
    InstanceConfigOverride::new(
        "/dns/sequencer-core-service.sequencer0-preintegration.svc.cluster.local/tcp/53080/p2p/\
         12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        "false",
        "0x0101010101010101010101010101010101010101010101010101010101010104",
        "/dns/sequencer-mempool-service.sequencer0-preintegration.svc.cluster.local/tcp/53200/p2p/\
         12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
        "false",
        "0x0101010101010101010101010101010101010101010101010101010101010104",
        "0x4",
    );

const TESTING_INSTANCE_CONFIG_OVERRIDE: InstanceConfigOverride = InstanceConfigOverride::new(
    "",
    "true",
    "0x0101010101010101010101010101010101010101010101010101010101010101",
    "",
    "true",
    "0x0101010101010101010101010101010101010101010101010101010101010101",
    "0x64",
);

const SEPOLIA_INTEGRATION_NODE_0_CONFIG_OVERRIDE: ConfigOverride = ConfigOverride::new(
    &INTEGRATION_DEPLOYMENT_CONFIG_OVERRIDE,
    &SEPOLIA_INTEGRATION_NODE_0_INSTANCE_CONFIG_OVERRIDE,
);
const SEPOLIA_INTEGRATION_NODE_1_CONFIG_OVERRIDE: ConfigOverride = ConfigOverride::new(
    &INTEGRATION_DEPLOYMENT_CONFIG_OVERRIDE,
    &SEPOLIA_INTEGRATION_NODE_1_INSTANCE_CONFIG_OVERRIDE,
);
const SEPOLIA_INTEGRATION_NODE_2_CONFIG_OVERRIDE: ConfigOverride = ConfigOverride::new(
    &INTEGRATION_DEPLOYMENT_CONFIG_OVERRIDE,
    &SEPOLIA_INTEGRATION_NODE_2_INSTANCE_CONFIG_OVERRIDE,
);
const SEPOLIA_INTEGRATION_NODE_3_CONFIG_OVERRIDE: ConfigOverride = ConfigOverride::new(
    &INTEGRATION_DEPLOYMENT_CONFIG_OVERRIDE,
    &SEPOLIA_INTEGRATION_NODE_3_INSTANCE_CONFIG_OVERRIDE,
);
const TESTING_CONFIG_OVERRIDE: ConfigOverride =
    ConfigOverride::new(&TESTING_DEPLOYMENT_CONFIG_OVERRIDE, &TESTING_INSTANCE_CONFIG_OVERRIDE);

type DeploymentFn = fn() -> Deployment;

// TODO(Tsabary): create deployment instances per per deployment.

pub const DEPLOYMENTS: &[DeploymentFn] = &[
    system_test_distributed_deployment,
    system_test_consolidated_deployment,
    integration_hybrid_deployment_node_0,
    integration_hybrid_deployment_node_1,
    integration_hybrid_deployment_node_2,
    integration_hybrid_deployment_node_3,
];

// Integration deployments

fn integration_hybrid_deployment_node_0() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::SepoliaIntegration,
        "integration_hybrid_node_0",
        Some(ExternalSecret::new("node-0-integration-secrets")),
        DEPLOYMENT_IMAGE_FOR_PRE_INTEGRATION,
        PathBuf::from(INTEGRATION_BASE_APP_CONFIG_PATH),
        SEPOLIA_INTEGRATION_NODE_0_CONFIG_OVERRIDE,
    )
}

fn integration_hybrid_deployment_node_1() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::SepoliaIntegration,
        "integration_hybrid_node_1",
        Some(ExternalSecret::new("node-1-integration-secrets")),
        DEPLOYMENT_IMAGE_FOR_PRE_INTEGRATION,
        PathBuf::from(INTEGRATION_BASE_APP_CONFIG_PATH),
        SEPOLIA_INTEGRATION_NODE_1_CONFIG_OVERRIDE,
    )
}

fn integration_hybrid_deployment_node_2() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::SepoliaIntegration,
        "integration_hybrid_node_2",
        Some(ExternalSecret::new("node-2-integration-secrets")),
        DEPLOYMENT_IMAGE_FOR_PRE_INTEGRATION,
        PathBuf::from(INTEGRATION_BASE_APP_CONFIG_PATH),
        SEPOLIA_INTEGRATION_NODE_2_CONFIG_OVERRIDE,
    )
}

fn integration_hybrid_deployment_node_3() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::HybridNode,
        Environment::SepoliaIntegration,
        "integration_hybrid_node_3",
        Some(ExternalSecret::new("node-3-integration-secrets")),
        DEPLOYMENT_IMAGE_FOR_PRE_INTEGRATION,
        PathBuf::from(INTEGRATION_BASE_APP_CONFIG_PATH),
        SEPOLIA_INTEGRATION_NODE_3_CONFIG_OVERRIDE,
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
        DEPLOYMENT_IMAGE_FOR_TESTING,
        PathBuf::from(SYSTEM_TEST_BASE_APP_CONFIG_PATH),
        TESTING_CONFIG_OVERRIDE,
    )
}

fn system_test_consolidated_deployment() -> Deployment {
    Deployment::new(
        ChainId::IntegrationSepolia,
        DeploymentName::ConsolidatedNode,
        Environment::Testing,
        "deployment_test_consolidated",
        None,
        DEPLOYMENT_IMAGE_FOR_TESTING,
        PathBuf::from(SYSTEM_TEST_BASE_APP_CONFIG_PATH),
        TESTING_CONFIG_OVERRIDE,
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
