use apollo_infra::component_client::{ClientError, LocalComponentReaderClient};
use apollo_node_config::node_config::NodeDynamicConfig;
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
