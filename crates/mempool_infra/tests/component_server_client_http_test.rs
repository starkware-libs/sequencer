mod common;

use async_trait::async_trait;
use common::{ComponentAClientTrait, ComponentBClientTrait, ResultA, ResultB};
use serde::{Deserialize, Serialize};
use starknet_mempool_infra::component_client::{ClientError, ComponentClientHttp};
use starknet_mempool_infra::component_definitions::ComponentRequestHandler;
use starknet_mempool_infra::component_server::ComponentServerHttp;
use tokio::task;

use crate::common::{ComponentA, ComponentB, ValueA, ValueB};

// Todo(uriel): Move to common
#[derive(Serialize, Deserialize, Debug)]
pub enum ComponentARequest {
    AGetValue,
}

// Todo(uriel): Move to common
#[derive(Serialize, Deserialize, Debug)]
pub enum ComponentAResponse {
    Value(ValueA),
}

#[async_trait]
impl ComponentAClientTrait for ComponentClientHttp<ComponentARequest, ComponentAResponse> {
    async fn a_get_value(&self) -> ResultA {
        match self.send(ComponentARequest::AGetValue).await? {
            ComponentAResponse::Value(value) => Ok(value),
        }
    }
}

#[async_trait]
impl ComponentRequestHandler<ComponentARequest, ComponentAResponse> for ComponentA {
    async fn handle_request(&mut self, request: ComponentARequest) -> ComponentAResponse {
        match request {
            ComponentARequest::AGetValue => ComponentAResponse::Value(self.a_get_value().await),
        }
    }
}

// Todo(uriel): Move to common
#[derive(Serialize, Deserialize, Debug)]
pub enum ComponentBRequest {
    BGetValue,
}

// Todo(uriel): Move to common
#[derive(Serialize, Deserialize, Debug)]
pub enum ComponentBResponse {
    Value(ValueB),
}

#[async_trait]
impl ComponentBClientTrait for ComponentClientHttp<ComponentBRequest, ComponentBResponse> {
    async fn b_get_value(&self) -> ResultB {
        match self.send(ComponentBRequest::BGetValue).await? {
            ComponentBResponse::Value(value) => Ok(value),
        }
    }
}

#[async_trait]
impl ComponentRequestHandler<ComponentBRequest, ComponentBResponse> for ComponentB {
    async fn handle_request(&mut self, request: ComponentBRequest) -> ComponentBResponse {
        match request {
            ComponentBRequest::BGetValue => ComponentBResponse::Value(self.b_get_value()),
        }
    }
}

async fn verify_response(
    a_client: ComponentClientHttp<ComponentARequest, ComponentAResponse>,
    expected_value: ValueA,
) {
    assert_eq!(a_client.a_get_value().await.unwrap(), expected_value);
}

async fn verify_error(
    a_client: ComponentClientHttp<ComponentARequest, ComponentAResponse>,
    expected_error: ClientError,
) {
    assert_eq!(a_client.a_get_value().await, Err(expected_error));
}

#[tokio::test]
async fn test_setup() {
    let setup_value: ValueB = 90;
    let expected_value: ValueA = setup_value.into();

    let local_ip = "::1".parse().unwrap();
    let a_port = 10000;
    let b_port = 10001;

    let a_client =
        ComponentClientHttp::<ComponentARequest, ComponentAResponse>::new(local_ip, a_port);
    let b_client =
        ComponentClientHttp::<ComponentBRequest, ComponentBResponse>::new(local_ip, b_port);

    verify_error(a_client.clone(), ClientError::CommunicationFailure).await;

    let component_a = ComponentA::new(Box::new(b_client));
    let component_b = ComponentB::new(setup_value, Box::new(a_client.clone()));

    let mut component_a_server = ComponentServerHttp::<
        ComponentA,
        ComponentARequest,
        ComponentAResponse,
    >::new(component_a, local_ip, a_port);
    let mut component_b_server = ComponentServerHttp::<
        ComponentB,
        ComponentBRequest,
        ComponentBResponse,
    >::new(component_b, local_ip, b_port);

    task::spawn(async move {
        component_a_server.start().await;
    });

    task::spawn(async move {
        component_b_server.start().await;
    });

    // Todo(uriel): Get rid of this
    task::yield_now().await;

    verify_response(a_client.clone(), expected_value).await;
}
