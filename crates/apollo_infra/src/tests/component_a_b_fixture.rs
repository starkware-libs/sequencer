use std::sync::Arc;

use apollo_metrics::generate_permutation_labels;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use starknet_types_core::felt::Felt;
use strum::{AsRefStr, EnumDiscriminants, EnumIter, IntoStaticStr, VariantNames};
use tokio::sync::Semaphore;

use crate::component_client::ClientResult;
use crate::component_definitions::{ComponentRequestHandler, ComponentStarter, PrioritizedRequest};
use crate::requests::LABEL_NAME_REQUEST_VARIANT;
use crate::{impl_debug_for_infra_requests_and_responses, impl_labeled_request};

pub(crate) type ValueA = Felt;
pub(crate) type ValueB = Felt;
pub(crate) type ResultA = ClientResult<ValueA>;
pub(crate) type ResultB = ClientResult<ValueB>;

pub(crate) const VALID_VALUE_A: ValueA = Felt::ONE;

#[derive(Serialize, Deserialize, Clone, AsRefStr, EnumDiscriminants)]
#[strum_discriminants(
    name(ComponentARequestLabelValue),
    derive(IntoStaticStr, EnumIter, VariantNames),
    strum(serialize_all = "snake_case")
)]
pub(crate) enum ComponentARequest {
    AGetValue,
}

generate_permutation_labels! {
    COMPONENT_A_REQUEST_LABELS,
    (LABEL_NAME_REQUEST_VARIANT, ComponentARequestLabelValue),
}

impl_debug_for_infra_requests_and_responses!(ComponentARequest);
impl_labeled_request!(ComponentARequest, ComponentARequestLabelValue);
impl PrioritizedRequest for ComponentARequest {}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum ComponentAResponse {
    AGetValue(ValueA),
}

#[derive(Serialize, Deserialize, Clone, AsRefStr, EnumDiscriminants)]
#[strum_discriminants(
    name(ComponentBRequestLabelValue),
    derive(IntoStaticStr, EnumIter, VariantNames),
    strum(serialize_all = "snake_case")
)]
pub(crate) enum ComponentBRequest {
    BGetValue,
    BSetValue(ValueB),
}

generate_permutation_labels! {
    COMPONENT_B_REQUEST_LABELS,
    (LABEL_NAME_REQUEST_VARIANT, ComponentBRequestLabelValue),
}

impl_debug_for_infra_requests_and_responses!(ComponentBRequest);
impl_labeled_request!(ComponentBRequest, ComponentBRequestLabelValue);
impl PrioritizedRequest for ComponentBRequest {}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum ComponentBResponse {
    BGetValue(ValueB),
    BSetValue,
}

#[async_trait]
pub(crate) trait ComponentAClientTrait: Send + Sync {
    async fn a_get_value(&self) -> ResultA;
}

#[async_trait]
pub(crate) trait ComponentBClientTrait: Send + Sync {
    async fn b_get_value(&self) -> ResultB;
    async fn b_set_value(&self, value: ValueB) -> ClientResult<()>;
}

pub(crate) struct ComponentA {
    b: Box<dyn ComponentBClientTrait>,
    sem: Option<Arc<Semaphore>>,
}

impl ComponentA {
    pub(crate) fn new(b: Box<dyn ComponentBClientTrait>) -> Self {
        Self { b, sem: None }
    }

    pub(crate) async fn a_get_value(&self) -> ValueA {
        self.b.b_get_value().await.unwrap()
    }

    pub(crate) fn with_semaphore(b: Box<dyn ComponentBClientTrait>, sem: Arc<Semaphore>) -> Self {
        Self { b, sem: Some(sem) }
    }
}

impl ComponentStarter for ComponentA {}

pub(crate) struct ComponentB {
    value: ValueB,
    _a: Box<dyn ComponentAClientTrait>,
}

impl ComponentB {
    pub(crate) fn new(value: ValueB, a: Box<dyn ComponentAClientTrait>) -> Self {
        Self { value, _a: a }
    }

    pub(crate) fn b_get_value(&self) -> ValueB {
        self.value
    }

    pub(crate) fn b_set_value(&mut self, value: ValueB) {
        self.value = value;
    }
}

impl ComponentStarter for ComponentB {}

pub(crate) async fn test_a_b_functionality(
    a_client: impl ComponentAClientTrait,
    b_client: impl ComponentBClientTrait,
    expected_value: ValueA,
) {
    assert_eq!(a_client.a_get_value().await.unwrap(), expected_value);

    let new_expected_value: ValueA = expected_value + 1;
    assert!(b_client.b_set_value(new_expected_value).await.is_ok());
    assert_eq!(a_client.a_get_value().await.unwrap(), new_expected_value);
}

#[async_trait]
impl ComponentRequestHandler<ComponentARequest, ComponentAResponse> for ComponentA {
    async fn handle_request(&mut self, request: ComponentARequest) -> ComponentAResponse {
        match request {
            ComponentARequest::AGetValue => {
                if let Some(sem) = &self.sem {
                    let _permit = sem.clone().acquire_owned().await.unwrap();
                    let v = self.a_get_value().await;
                    ComponentAResponse::AGetValue(v)
                } else {
                    ComponentAResponse::AGetValue(self.a_get_value().await)
                }
            }
        }
    }
}

#[async_trait]
impl ComponentRequestHandler<ComponentBRequest, ComponentBResponse> for ComponentB {
    async fn handle_request(&mut self, request: ComponentBRequest) -> ComponentBResponse {
        match request {
            ComponentBRequest::BGetValue => ComponentBResponse::BGetValue(self.b_get_value()),
            ComponentBRequest::BSetValue(value) => {
                self.b_set_value(value);
                ComponentBResponse::BSetValue
            }
        }
    }
}
