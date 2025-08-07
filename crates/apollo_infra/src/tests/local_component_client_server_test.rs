use async_trait::async_trait;
use starknet_types_core::felt::Felt;
use tokio::sync::mpsc::channel;
use tokio::task;

use crate::component_client::{ClientError, ClientResult, LocalComponentClient};
use crate::component_definitions::{ComponentClient, ComponentRequestAndResponseSender};
use crate::component_server::{ComponentServerStarter, LocalComponentServer};
use crate::tests::{
    test_a_b_functionality,
    ComponentA,
    ComponentAClientTrait,
    ComponentARequest,
    ComponentAResponse,
    ComponentB,
    ComponentBClientTrait,
    ComponentBRequest,
    ComponentBResponse,
    ResultA,
    ResultB,
    ValueA,
    ValueB,
    TEST_LOCAL_SERVER_METRICS,
};

type ComponentAClient = LocalComponentClient<ComponentARequest, ComponentAResponse>;
type ComponentBClient = LocalComponentClient<ComponentBRequest, ComponentBResponse>;

#[async_trait]
impl ComponentAClientTrait for LocalComponentClient<ComponentARequest, ComponentAResponse> {
    async fn a_get_value(&self) -> ResultA {
        let res = self.send(ComponentARequest::AGetValue).await;
        match res? {
            ComponentAResponse::AGetValue(value) => Ok(value),
        }
    }
}

#[async_trait]
impl ComponentBClientTrait for LocalComponentClient<ComponentBRequest, ComponentBResponse> {
    async fn b_get_value(&self) -> ResultB {
        let res = self.send(ComponentBRequest::BGetValue).await;
        match res? {
            ComponentBResponse::BGetValue(value) => Ok(value),
            unexpected_response => {
                Err(ClientError::UnexpectedResponse(format!("{unexpected_response:?}")))
            }
        }
    }

    async fn b_set_value(&self, value: ValueB) -> ClientResult<()> {
        let res = self.send(ComponentBRequest::BSetValue(value)).await;
        match res? {
            ComponentBResponse::BSetValue => Ok(()),
            unexpected_response => {
                Err(ClientError::UnexpectedResponse(format!("{unexpected_response:?}")))
            }
        }
    }
}

#[tokio::test]
async fn local_client_server() {
    let setup_value: ValueB = Felt::from(30);
    let expected_value: ValueA = setup_value;

    let (tx_a, rx_a) =
        channel::<ComponentRequestAndResponseSender<ComponentARequest, ComponentAResponse>>(32);
    let (tx_b, rx_b) =
        channel::<ComponentRequestAndResponseSender<ComponentBRequest, ComponentBResponse>>(32);

    let a_client = ComponentAClient::new(tx_a.clone());
    let b_client = ComponentBClient::new(tx_b.clone());

    let component_a = ComponentA::new(Box::new(b_client.clone()));
    let component_b = ComponentB::new(setup_value, Box::new(a_client.clone()));

    let mut component_a_server =
        LocalComponentServer::new(component_a, rx_a, &TEST_LOCAL_SERVER_METRICS);
    let mut component_b_server =
        LocalComponentServer::new(component_b, rx_b, &TEST_LOCAL_SERVER_METRICS);

    task::spawn(async move {
        let _ = component_a_server.start().await;
    });

    task::spawn(async move {
        let _ = component_b_server.start().await;
    });

    test_a_b_functionality(a_client, b_client, expected_value).await;
}
