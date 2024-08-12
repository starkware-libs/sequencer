mod common;

use std::net::{IpAddr, Ipv6Addr};

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
use starknet_mempool_infra::component_client::{
    ClientError,
    ClientResult,
    TCPRemoteComponentClient,
};
use starknet_mempool_infra::component_definitions::ComponentRequestHandler;
use starknet_mempool_infra::component_server::{ComponentServerStarter, TCPRemoteComponentServer};
use tokio::task;

type ComponentAClient = TCPRemoteComponentClient<ComponentARequest, ComponentAResponse>;
type ComponentBClient = TCPRemoteComponentClient<ComponentBRequest, ComponentBResponse>;

use crate::common::{test_a_b_functionality, ComponentA, ComponentB, ValueB};

const LOCAL_IP: IpAddr = IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1));
const A_PORT_TEST_SETUP: u16 = 10000;
const B_PORT_TEST_SETUP: u16 = 10001;

#[async_trait]
impl ComponentAClientTrait for ComponentAClient {
    async fn a_get_value(&self) -> ResultA {
        match self.send(ComponentARequest::AGetValue).await? {
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
impl ComponentBClientTrait for ComponentBClient {
    async fn b_get_value(&self) -> ResultB {
        match self.send(ComponentBRequest::BGetValue).await? {
            ComponentBResponse::BGetValue(value) => Ok(value),
            unexpected_response => {
                Err(ClientError::UnexpectedResponse(format!("{unexpected_response:?}")))
            }
        }
    }

    async fn b_set_value(&self, value: ValueB) -> ClientResult<()> {
        match self.send(ComponentBRequest::BSetValue(value)).await? {
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

async fn setup_for_tests(setup_value: ValueB, a_port: u16, b_port: u16) {
    let a_client = ComponentAClient::new(LOCAL_IP, a_port);
    let b_client = ComponentBClient::new(LOCAL_IP, b_port);

    let component_a = ComponentA::new(Box::new(b_client));
    let component_b = ComponentB::new(setup_value, Box::new(a_client));

    let mut component_a_server = TCPRemoteComponentServer::<
        ComponentA,
        ComponentARequest,
        ComponentAResponse,
    >::new(component_a, LOCAL_IP, a_port);
    let mut component_b_server = TCPRemoteComponentServer::<
        ComponentB,
        ComponentBRequest,
        ComponentBResponse,
    >::new(component_b, LOCAL_IP, b_port);

    task::spawn(async move {
        component_a_server.start().await;
    });

    task::spawn(async move {
        component_b_server.start().await;
    });

    // Todo(uriel): Get rid of this
    task::yield_now().await;
}

#[tokio::test]
async fn test_proper_setup() {
    let setup_value: ValueB = 90;
    setup_for_tests(setup_value, A_PORT_TEST_SETUP, B_PORT_TEST_SETUP).await;
    let a_client = ComponentAClient::new(LOCAL_IP, A_PORT_TEST_SETUP);
    let b_client = ComponentBClient::new(LOCAL_IP, B_PORT_TEST_SETUP);

    test_a_b_functionality(a_client, b_client, setup_value.into()).await;
}
