mod common;

use async_trait::async_trait;
use common::{
    ComponentAClientTrait,
    ComponentARequest,
    ComponentAResponse,
    ComponentBClientTrait,
    ComponentBRequest,
    ComponentBResponse,
    ResultA,
    ResultB,
};
use starknet_mempool_infra::component_client::{ClientError, ClientResult, LocalComponentClient};
use starknet_mempool_infra::component_definitions::{
    ComponentRequestAndResponseSender,
    ComponentRequestHandler,
};
use starknet_mempool_infra::component_server::{ComponentServerStarter, LocalComponentServer};
use tokio::sync::mpsc::channel;
use tokio::task;

use crate::common::{verify_response, ComponentA, ComponentB, ValueA, ValueB};

type ComponentAClient = LocalComponentClient<ComponentARequest, ComponentAResponse>;
type ComponentBClient = LocalComponentClient<ComponentBRequest, ComponentBResponse>;

// TODO(Tsabary): send messages from component b to component a.

#[async_trait]
impl ComponentAClientTrait for LocalComponentClient<ComponentARequest, ComponentAResponse> {
    async fn a_get_value(&self) -> ResultA {
        let res = self.send(ComponentARequest::AGetValue).await;
        match res {
            ComponentAResponse::AGetValue(value) => Ok(value),
        }
    }
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
impl ComponentBClientTrait for LocalComponentClient<ComponentBRequest, ComponentBResponse> {
    async fn b_get_value(&self) -> ResultB {
        let res = self.send(ComponentBRequest::BGetValue).await;
        match res {
            ComponentBResponse::BGetValue(value) => Ok(value),
            unexpected_response => {
                Err(ClientError::UnexpectedResponse(format!("{unexpected_response:?}")))
            }
        }
    }

    async fn b_set_value(&self, value: ValueB) -> ClientResult<()> {
        match self.send(ComponentBRequest::BSetValue(value)).await {
            ComponentBResponse::BSetValue => Ok(()),
            unexpected_response => {
                Err(ClientError::UnexpectedResponse(format!("{unexpected_response:?}")))
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

#[tokio::test]
async fn test_setup() {
    let setup_value: ValueB = 30;
    let expected_value: ValueA = setup_value.into();

    let (tx_a, rx_a) =
        channel::<ComponentRequestAndResponseSender<ComponentARequest, ComponentAResponse>>(32);
    let (tx_b, rx_b) =
        channel::<ComponentRequestAndResponseSender<ComponentBRequest, ComponentBResponse>>(32);

    let a_client = ComponentAClient::new(tx_a.clone());
    let b_client = ComponentBClient::new(tx_b.clone());

    let component_a = ComponentA::new(Box::new(b_client.clone()));
    let component_b = ComponentB::new(setup_value, Box::new(a_client.clone()));

    let mut component_a_server = LocalComponentServer::new(component_a, rx_a);
    let mut component_b_server = LocalComponentServer::new(component_b, rx_b);

    task::spawn(async move {
        component_a_server.start().await;
    });

    task::spawn(async move {
        component_b_server.start().await;
    });

    verify_response(a_client, b_client, expected_value).await;
}
