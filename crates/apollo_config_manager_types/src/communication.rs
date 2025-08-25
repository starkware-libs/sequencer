use std::sync::Arc;

use apollo_infra::component_client::{LocalComponentClient, RemoteComponentClient};
use apollo_infra::component_definitions::{ComponentClient, PrioritizedRequest, RequestWrapper};
use apollo_infra::{impl_debug_for_infra_requests_and_responses, impl_labeled_request};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use strum::EnumVariantNames;
use strum_macros::{AsRefStr, EnumDiscriminants, EnumIter, IntoStaticStr};

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
    // TODO(Nadin): Remove this placeholder when adding real request variants
    #[allow(dead_code)]
    Placeholder,
}
impl_debug_for_infra_requests_and_responses!(ConfigManagerRequest);
impl_labeled_request!(ConfigManagerRequest, ConfigManagerRequestLabelValue);
impl PrioritizedRequest for ConfigManagerRequest {}

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum ConfigManagerResponse {
    // TODO(Nadin): Remove this placeholder when adding real response variants
    #[allow(dead_code)]
    Placeholder,
}
impl_debug_for_infra_requests_and_responses!(ConfigManagerResponse);

pub enum ConfigManagerClientError {}

#[async_trait]
impl<ComponentClientType> ConfigManagerClient for ComponentClientType where
    ComponentClientType: Send + Sync + ComponentClient<ConfigManagerRequest, ConfigManagerResponse>
{
}
