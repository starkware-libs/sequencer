use async_trait::async_trait;
use starknet_types_core::felt::Felt;
use tokio::sync::mpsc::channel;
use tokio::task;

use crate::component_client::{
    ClientError,
    ClientResult,
    LocalClientConfig,
    LocalComponentClient,
    REQUEST_TIMEOUT_ERROR_MESSAGE,
};
use crate::component_definitions::{ComponentClient, RequestWrapper};
use crate::component_server::{ComponentServerStarter, LocalComponentServer, LocalServerConfig};
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
    TEST_LOCAL_CLIENT_METRICS,
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

    let (tx_a, rx_a) = channel::<RequestWrapper<ComponentARequest, ComponentAResponse>>(32);
    let (tx_b, rx_b) = channel::<RequestWrapper<ComponentBRequest, ComponentBResponse>>(32);

    let a_client = ComponentAClient::new(
        LocalClientConfig::default(),
        tx_a.clone(),
        &TEST_LOCAL_CLIENT_METRICS,
    );
    let b_client = ComponentBClient::new(
        LocalClientConfig::default(),
        tx_b.clone(),
        &TEST_LOCAL_CLIENT_METRICS,
    );

    let component_a = ComponentA::new(Box::new(b_client.clone()));
    let component_b = ComponentB::new(setup_value, Box::new(a_client.clone()));

    let config = LocalServerConfig::default();
    let mut component_a_server =
        LocalComponentServer::new(component_a, &config, rx_a, &TEST_LOCAL_SERVER_METRICS);
    let mut component_b_server =
        LocalComponentServer::new(component_b, &config, rx_b, &TEST_LOCAL_SERVER_METRICS);

    task::spawn(async move {
        let _ = component_a_server.start().await;
    });

    task::spawn(async move {
        let _ = component_b_server.start().await;
    });

    test_a_b_functionality(a_client, b_client, expected_value).await;
}

/// "Server" receives the request but never sends a response. Verifies the client times out and
/// returns CommunicationFailure("request timed out").
#[tokio::test]
async fn request_times_out_when_server_never_responds() {
    let (tx, mut rx) = channel::<RequestWrapper<ComponentARequest, ComponentAResponse>>(1);
    task::spawn(async move {
        // Receive one request and never respond (hold wrapper, never send on res_tx).
        let _wrapper = rx.recv().await;
        std::future::pending::<()>().await
    });
    task::yield_now().await;

    let timeout_config = LocalClientConfig { request_timeout_ms: 200, ..Default::default() };
    let client = ComponentAClient::new(timeout_config, tx, &TEST_LOCAL_CLIENT_METRICS);

    let Err(e) = client.a_get_value().await else {
        panic!("Expected an error");
    };
    assert!(
        e.to_string().contains(REQUEST_TIMEOUT_ERROR_MESSAGE),
        "Expected error to contain '{REQUEST_TIMEOUT_ERROR_MESSAGE}', got: {e}"
    );
}
