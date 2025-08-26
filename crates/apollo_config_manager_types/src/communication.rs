use std::sync::Arc;

use apollo_infra::component_client::{LocalComponentClient, RemoteComponentClient};
use apollo_infra::component_definitions::{ComponentClient, PrioritizedRequest, RequestWrapper};
use apollo_infra::{impl_debug_for_infra_requests_and_responses, impl_labeled_request};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum::EnumVariantNames;
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
pub trait ConfigManagerClient: Send + Sync {}

#[derive(Serialize, Deserialize, Clone, AsRefStr, EnumDiscriminants)]
#[strum_discriminants(
    name(ConfigManagerRequestLabelValue),
    derive(IntoStaticStr, EnumIter, EnumVariantNames),
    strum(serialize_all = "snake_case")
)]
pub enum ConfigManagerRequest {
    ReadConfig,
}
impl_debug_for_infra_requests_and_responses!(ConfigManagerRequest);
impl_labeled_request!(ConfigManagerRequest, ConfigManagerRequestLabelValue);
impl PrioritizedRequest for ConfigManagerRequest {}

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum ConfigManagerResponse {
    ReadConfig(ConfigManagerResult<Value>),
}
impl_debug_for_infra_requests_and_responses!(ConfigManagerResponse);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConfigManagerClientError {}

#[async_trait]
impl<ComponentClientType> ConfigManagerClient for ComponentClientType where
    ComponentClientType: Send + Sync + ComponentClient<ConfigManagerRequest, ConfigManagerResponse>
{
}
