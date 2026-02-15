use std::sync::Arc;

use apollo_batcher_config::config::BatcherDynamicConfig;
use apollo_consensus_config::config::ConsensusDynamicConfig;
use apollo_consensus_orchestrator_config::config::ContextDynamicConfig;
use apollo_http_server_config::config::HttpServerDynamicConfig;
use apollo_infra::component_client::{ClientError, LocalComponentClient, RemoteComponentClient};
use apollo_infra::component_definitions::{ComponentClient, PrioritizedRequest, RequestWrapper};
use apollo_infra::{
    handle_all_response_variants,
    impl_debug_for_infra_requests_and_responses,
    impl_labeled_request,
};
use apollo_mempool_config::config::MempoolDynamicConfig;
use apollo_metrics::generate_permutation_labels;
use apollo_node_config::node_config::NodeDynamicConfig;
use apollo_staking_config::config::StakingManagerDynamicConfig;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use strum::{EnumVariantNames, VariantNames};
use strum_macros::{AsRefStr, EnumDiscriminants, EnumIter, IntoStaticStr};
use thiserror::Error;

use crate::config_manager_types::ConfigManagerResult;
use crate::errors::ConfigManagerError;

pub type LocalConfigManagerClient =
    LocalComponentClient<ConfigManagerRequest, ConfigManagerResponse>;
pub type RemoteConfigManagerClient =
    RemoteComponentClient<ConfigManagerRequest, ConfigManagerResponse>;
pub type ConfigManagerClientResult<T> = Result<T, ConfigManagerClientError>;
pub type ConfigManagerRequestWrapper = RequestWrapper<ConfigManagerRequest, ConfigManagerResponse>;
pub type SharedConfigManagerClient = Arc<dyn ConfigManagerClient>;

#[cfg_attr(any(feature = "testing", test), mockall::automock)]
#[async_trait]
pub trait ConfigManagerClient: Send + Sync {
    async fn get_consensus_dynamic_config(
        &self,
    ) -> ConfigManagerClientResult<ConsensusDynamicConfig>;

    async fn get_context_dynamic_config(&self) -> ConfigManagerClientResult<ContextDynamicConfig>;
    async fn get_http_server_dynamic_config(
        &self,
    ) -> ConfigManagerClientResult<HttpServerDynamicConfig>;

    async fn get_mempool_dynamic_config(&self) -> ConfigManagerClientResult<MempoolDynamicConfig>;
    async fn get_batcher_dynamic_config(&self) -> ConfigManagerClientResult<BatcherDynamicConfig>;
    async fn get_staking_manager_dynamic_config(
        &self,
    ) -> ConfigManagerClientResult<StakingManagerDynamicConfig>;

    async fn set_node_dynamic_config(
        &self,
        config: NodeDynamicConfig,
    ) -> ConfigManagerClientResult<()>;
}

#[derive(Serialize, Deserialize, Clone, AsRefStr, EnumDiscriminants)]
#[strum_discriminants(
    name(ConfigManagerRequestLabelValue),
    derive(IntoStaticStr, EnumIter, EnumVariantNames),
    strum(serialize_all = "snake_case")
)]
pub enum ConfigManagerRequest {
    GetConsensusDynamicConfig,
    GetContextDynamicConfig,
    GetHttpServerDynamicConfig,
    GetMempoolDynamicConfig,
    GetBatcherDynamicConfig,
    GetStakingManagerDynamicConfig,
    SetNodeDynamicConfig(Box<NodeDynamicConfig>),
}
impl_debug_for_infra_requests_and_responses!(ConfigManagerRequest);
impl_labeled_request!(ConfigManagerRequest, ConfigManagerRequestLabelValue);
impl PrioritizedRequest for ConfigManagerRequest {}

const CONFIG_MANAGER_REQUEST_TYPE_LABEL: &str = "request_type";

generate_permutation_labels! {
    CONFIG_MANAGER_REQUEST_LABELS,
    (CONFIG_MANAGER_REQUEST_TYPE_LABEL, ConfigManagerRequestLabelValue),
}

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum ConfigManagerResponse {
    GetConsensusDynamicConfig(ConfigManagerResult<ConsensusDynamicConfig>),
    GetContextDynamicConfig(ConfigManagerResult<ContextDynamicConfig>),
    GetHttpServerDynamicConfig(ConfigManagerResult<HttpServerDynamicConfig>),
    GetMempoolDynamicConfig(ConfigManagerResult<MempoolDynamicConfig>),
    GetBatcherDynamicConfig(ConfigManagerResult<BatcherDynamicConfig>),
    GetStakingManagerDynamicConfig(ConfigManagerResult<StakingManagerDynamicConfig>),
    SetNodeDynamicConfig(ConfigManagerResult<()>),
}
impl_debug_for_infra_requests_and_responses!(ConfigManagerResponse);

#[derive(Clone, Debug, Error)]
pub enum ConfigManagerClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    ConfigManagerError(#[from] ConfigManagerError),
}

#[async_trait]
impl<ComponentClientType> ConfigManagerClient for ComponentClientType
where
    ComponentClientType: Send + Sync + ComponentClient<ConfigManagerRequest, ConfigManagerResponse>,
{
    async fn get_consensus_dynamic_config(
        &self,
    ) -> ConfigManagerClientResult<ConsensusDynamicConfig> {
        let request = ConfigManagerRequest::GetConsensusDynamicConfig;
        handle_all_response_variants!(
            self,
            request,
            ConfigManagerResponse,
            GetConsensusDynamicConfig,
            ConfigManagerClientError,
            ConfigManagerError,
            Direct
        )
    }

    async fn get_context_dynamic_config(&self) -> ConfigManagerClientResult<ContextDynamicConfig> {
        let request = ConfigManagerRequest::GetContextDynamicConfig;
        handle_all_response_variants!(
            self,
            request,
            ConfigManagerResponse,
            GetContextDynamicConfig,
            ConfigManagerClientError,
            ConfigManagerError,
            Direct
        )
    }

    async fn get_http_server_dynamic_config(
        &self,
    ) -> ConfigManagerClientResult<HttpServerDynamicConfig> {
        let request = ConfigManagerRequest::GetHttpServerDynamicConfig;
        handle_all_response_variants!(
            self,
            request,
            ConfigManagerResponse,
            GetHttpServerDynamicConfig,
            ConfigManagerClientError,
            ConfigManagerError,
            Direct
        )
    }

    async fn get_mempool_dynamic_config(&self) -> ConfigManagerClientResult<MempoolDynamicConfig> {
        let request = ConfigManagerRequest::GetMempoolDynamicConfig;
        handle_all_response_variants!(
            self,
            request,
            ConfigManagerResponse,
            GetMempoolDynamicConfig,
            ConfigManagerClientError,
            ConfigManagerError,
            Direct
        )
    }

    async fn get_batcher_dynamic_config(&self) -> ConfigManagerClientResult<BatcherDynamicConfig> {
        let request = ConfigManagerRequest::GetBatcherDynamicConfig;
        handle_all_response_variants!(
            self,
            request,
            ConfigManagerResponse,
            GetBatcherDynamicConfig,
            ConfigManagerClientError,
            ConfigManagerError,
            Direct
        )
    }

    async fn get_staking_manager_dynamic_config(
        &self,
    ) -> ConfigManagerClientResult<StakingManagerDynamicConfig> {
        let request = ConfigManagerRequest::GetStakingManagerDynamicConfig;
        handle_all_response_variants!(
            self,
            request,
            ConfigManagerResponse,
            GetStakingManagerDynamicConfig,
            ConfigManagerClientError,
            ConfigManagerError,
            Direct
        )
    }

    async fn set_node_dynamic_config(
        &self,
        config: NodeDynamicConfig,
    ) -> ConfigManagerClientResult<()> {
        let request = ConfigManagerRequest::SetNodeDynamicConfig(Box::new(config));
        handle_all_response_variants!(
            self,
            request,
            ConfigManagerResponse,
            SetNodeDynamicConfig,
            ConfigManagerClientError,
            ConfigManagerError,
            Direct
        )
    }
}
