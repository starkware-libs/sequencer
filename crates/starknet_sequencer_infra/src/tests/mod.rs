mod local_component_client_server_test;
mod remote_component_client_server_test;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use starknet_types_core::felt::Felt;

use crate::component_client::ClientResult;
use crate::component_definitions::{ComponentRequestHandler, ComponentStarter};
pub(crate) type ValueA = Felt;
pub(crate) type ValueB = Felt;
pub(crate) type ResultA = ClientResult<ValueA>;
pub(crate) type ResultB = ClientResult<ValueB>;

#[derive(Serialize, Deserialize, Debug)]
pub enum ComponentARequest {
    AGetValue,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ComponentAResponse {
    AGetValue(ValueA),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ComponentBRequest {
    BGetValue,
    BSetValue(ValueB),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ComponentBResponse {
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
}

impl ComponentA {
    pub fn new(b: Box<dyn ComponentBClientTrait>) -> Self {
        Self { b }
    }

    pub async fn a_get_value(&self) -> ValueA {
        self.b.b_get_value().await.unwrap()
    }
}

impl ComponentStarter for ComponentA {}

pub(crate) struct ComponentB {
    value: ValueB,
    _a: Box<dyn ComponentAClientTrait>,
}

impl ComponentB {
    pub fn new(value: ValueB, a: Box<dyn ComponentAClientTrait>) -> Self {
        Self { value, _a: a }
    }

    pub fn b_get_value(&self) -> ValueB {
        self.value
    }

    pub fn b_set_value(&mut self, value: ValueB) {
        self.value = value;
    }
}

impl ComponentStarter for ComponentB {}

pub(crate) async fn test_a_b_functionality(
    a_client: impl ComponentAClientTrait,
    b_client: impl ComponentBClientTrait,
    expected_value: ValueA,
) {
    // Check the setup value in component B through client A.
    assert_eq!(a_client.a_get_value().await.unwrap(), expected_value);

    let new_expected_value: ValueA = expected_value + 1;
    // Check that setting a new value to component B succeeds.
    assert!(b_client.b_set_value(new_expected_value).await.is_ok());
    // Check the new value in component B through client A.
    assert_eq!(a_client.a_get_value().await.unwrap(), new_expected_value);
}

#[async_trait]
impl ComponentRequestHandler<ComponentARequest, ComponentAResponse> for ComponentA {
    async fn handle_request(&mut self, request: ComponentARequest) -> ComponentAResponse {
        match request {
            ComponentARequest::AGetValue => ComponentAResponse::AGetValue(self.a_get_value().await),
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
