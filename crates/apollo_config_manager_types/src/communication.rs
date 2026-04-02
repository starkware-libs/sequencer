use std::sync::Arc;

use apollo_batcher_config::config::BatcherDynamicConfig;
use apollo_class_manager_config::config::ClassManagerDynamicConfig;
use apollo_consensus_config::config::ConsensusDynamicConfig;
use apollo_consensus_orchestrator_config::config::ContextDynamicConfig;
use apollo_http_server_config::config::HttpServerDynamicConfig;
use apollo_infra::component_client::ClientError;
use apollo_infra::component_definitions::ComponentChannelClient;
use apollo_mempool_config::config::MempoolDynamicConfig;
use apollo_node_config::node_config::NodeDynamicConfig;
use apollo_staking_config::config::StakingManagerDynamicConfig;
use apollo_state_sync_config::config::StateSyncDynamicConfig;
use thiserror::Error;

use crate::errors::ConfigManagerError;

pub type ConfigManagerClientResult<T> = Result<T, ConfigManagerClientError>;
pub type SharedConfigManagerChannelClient = Arc<dyn ConfigManagerChannelClient>;

#[derive(Clone, Debug, Error)]
pub enum ConfigManagerClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    ConfigManagerError(#[from] ConfigManagerError),
}

#[cfg_attr(any(feature = "testing", test), mockall::automock)]
pub trait ConfigManagerChannelClient: Send + Sync {
    fn get_consensus_dynamic_config(&self) -> ConfigManagerClientResult<ConsensusDynamicConfig>;
    fn get_class_manager_dynamic_config(
        &self,
    ) -> ConfigManagerClientResult<ClassManagerDynamicConfig>;
    fn get_context_dynamic_config(&self) -> ConfigManagerClientResult<ContextDynamicConfig>;
    fn get_http_server_dynamic_config(&self) -> ConfigManagerClientResult<HttpServerDynamicConfig>;
    fn get_mempool_dynamic_config(&self) -> ConfigManagerClientResult<MempoolDynamicConfig>;
    fn get_batcher_dynamic_config(&self) -> ConfigManagerClientResult<BatcherDynamicConfig>;
    fn get_state_sync_dynamic_config(&self) -> ConfigManagerClientResult<StateSyncDynamicConfig>;
    fn get_staking_manager_dynamic_config(
        &self,
    ) -> ConfigManagerClientResult<StakingManagerDynamicConfig>;
}

// Generates a `ConfigManagerChannelClient` method that reads a field from `NodeDynamicConfig`.
// The method name is derived by prepending `get_` to the field name.
macro_rules! impl_channel_client_getter {
    ($field:ident, $return_type:ty) => {
        paste::paste! {
            fn [<get_ $field>](&self) -> ConfigManagerClientResult<$return_type> {
                Ok(self.get_value().$field.expect("dynamic config for this component is not set"))
            }
        }
    };
}

impl<ComponentClientType> ConfigManagerChannelClient for ComponentClientType
where
    ComponentClientType: Send + Sync + ComponentChannelClient<NodeDynamicConfig>,
{
    impl_channel_client_getter!(batcher_dynamic_config, BatcherDynamicConfig);
    impl_channel_client_getter!(class_manager_dynamic_config, ClassManagerDynamicConfig);
    impl_channel_client_getter!(consensus_dynamic_config, ConsensusDynamicConfig);
    impl_channel_client_getter!(context_dynamic_config, ContextDynamicConfig);
    impl_channel_client_getter!(http_server_dynamic_config, HttpServerDynamicConfig);
    impl_channel_client_getter!(mempool_dynamic_config, MempoolDynamicConfig);
    impl_channel_client_getter!(state_sync_dynamic_config, StateSyncDynamicConfig);
    impl_channel_client_getter!(staking_manager_dynamic_config, StakingManagerDynamicConfig);
}
