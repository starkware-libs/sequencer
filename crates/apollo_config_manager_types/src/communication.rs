use std::sync::Arc;

use apollo_infra::component_client::{LocalComponentClient, RemoteComponentClient};
use apollo_infra::component_definitions::{ComponentClient, PrioritizedRequest, RequestWrapper};
use apollo_infra::{impl_debug_for_infra_requests_and_responses, impl_labeled_request};
use apollo_metrics::generate_permutation_labels;
use apollo_node_config::node_config::NodeDynamicConfig;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum::{EnumVariantNames, VariantNames};
use strum_macros::{AsRefStr, EnumDiscriminants, EnumIter, IntoStaticStr};

use crate::config_manager_types::ConfigManagerResult;

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
    async fn update_dynamic_config(
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
    ReadConfig,
    GetNodeDynamicConfig,
    UpdateDynamicConfig(NodeDynamicConfig),
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
    ReadConfig(ConfigManagerResult<Value>),
    GetNodeDynamicConfig(ConfigManagerResult<NodeDynamicConfig>),
    UpdateDynamicConfig(ConfigManagerResult<()>),
}
impl_debug_for_infra_requests_and_responses!(ConfigManagerResponse);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConfigManagerClientError {
    UpdateFailed,
    UnexpectedResponse,
}

#[async_trait]
impl<ComponentClientType> ConfigManagerClient for ComponentClientType
where
    ComponentClientType: Send + Sync + ComponentClient<ConfigManagerRequest, ConfigManagerResponse>,
{
    async fn update_dynamic_config(
        &self,
        config: NodeDynamicConfig,
    ) -> ConfigManagerClientResult<()> {
        let request = ConfigManagerRequest::UpdateDynamicConfig(config);
        let response = self.send(request).await;
        match response {
            Ok(ConfigManagerResponse::UpdateDynamicConfig(result)) => match result {
                Ok(()) => Ok(()),
                Err(_e) => Err(ConfigManagerClientError::UpdateFailed),
            },
            Ok(_) => Err(ConfigManagerClientError::UnexpectedResponse),
            Err(_e) => Err(ConfigManagerClientError::UpdateFailed),
        }
    }
}
