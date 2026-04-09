use apollo_batcher_config::config::BatcherDynamicConfig;
use apollo_class_manager_config::config::ClassManagerDynamicConfig;
use apollo_consensus_config::config::ConsensusDynamicConfig;
use apollo_consensus_orchestrator_config::config::ContextDynamicConfig;
use apollo_http_server_config::config::HttpServerDynamicConfig;
use apollo_infra::component_client::{ClientError, LocalComponentReaderClient};
use apollo_mempool_config::config::MempoolDynamicConfig;
use apollo_node_config::node_config::NodeDynamicConfig;
use apollo_staking_config::config::StakingManagerDynamicConfig;
use apollo_state_sync_config::config::StateSyncDynamicConfig;
use thiserror::Error;

use crate::errors::ConfigManagerError;

pub type ConfigManagerClientResult<T> = Result<T, ConfigManagerClientError>;
pub type LocalConfigManagerReaderClient = LocalComponentReaderClient<NodeDynamicConfig>;

#[derive(Clone, Debug, Error)]
pub enum ConfigManagerClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    ConfigManagerError(#[from] ConfigManagerError),
}
