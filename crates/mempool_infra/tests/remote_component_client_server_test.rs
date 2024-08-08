mod common;

use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::sync::Arc;

use async_trait::async_trait;
use bincode::{deserialize, serialize};
use common::{
    ComponentAClientTrait,
    ComponentARequest,
    ComponentAResponse,
    ComponentBClientTrait,
    ComponentBRequest,
    ComponentBResponse,
    ResultA,
    ResultB,
    ValueA,
};
use hyper::body::to_bytes;
use hyper::header::CONTENT_TYPE;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Request, Response, Server, StatusCode, Uri};
use rstest::rstest;
use serde::Serialize;
use starknet_mempool_infra::component_client::{ClientError, ClientResult, RemoteComponentClient};
use starknet_mempool_infra::component_definitions::{
    ComponentRequestHandler,
    ServerError,
    APPLICATION_OCTET_STREAM,
};
use starknet_mempool_infra::component_server::{ComponentServerStarter, RemoteComponentServer};
use tokio::sync::Mutex;
use tokio::task;

type ComponentAClient = RemoteComponentClient<ComponentARequest, ComponentAResponse>;
type ComponentBClient = RemoteComponentClient<ComponentBRequest, ComponentBResponse>;

use crate::common::{test_a_b_functionality, ComponentA, ComponentB, ValueB};

const LOCAL_IP: IpAddr = IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1));
const MAX_RETRIES: usize = 0;
const A_PORT_TEST_SETUP: u16 = 10000;
const B_PORT_TEST_SETUP: u16 = 10001;
const A_PORT_FAULTY_CLIENT: u16 = 10010;
const B_PORT_FAULTY_CLIENT: u16 = 10011;
const UNCONNECTED_SERVER_PORT: u16 = 10002;
const FAULTY_SERVER_REQ_DESER_PORT: u16 = 10003;
const FAULTY_SERVER_RES_DESER_PORT: u16 = 10004;
const RETRY_REQ_PORT: u16 = 10005;
const MOCK_SERVER_ERROR: &str = "mock server error";
const ARBITRARY_DATA: &str = "arbitrary data";
// ServerError::RequestDeserializationFailure error message.
const DESERIALIZE_REQ_ERROR_MESSAGE: &str = "Could not deserialize client request";
// ClientError::ResponseDeserializationFailure error message.
const DESERIALIZE_RES_ERROR_MESSAGE: &str = "Could not deserialize server response";
const VALID_VALUE_A: ValueA = 1;

#[async_trait]
impl ComponentAClientTrait for RemoteComponentClient<ComponentARequest, ComponentAResponse> {
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
impl ComponentBClientTrait for RemoteComponentClient<ComponentBRequest, ComponentBResponse> {
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

async fn verify_error(
    a_client: impl ComponentAClientTrait,
    expected_error_contained_keywords: &[&str],
) {
    let Err(error) = a_client.a_get_value().await else {
        panic!("Expected an error.");
    };
    assert_error_contains_keywords(error.to_string(), expected_error_contained_keywords)
}

fn assert_error_contains_keywords(error: String, expected_error_contained_keywords: &[&str]) {
    for expected_keyword in expected_error_contained_keywords {
        if !error.contains(expected_keyword) {
            panic!("Expected keyword: '{expected_keyword}' is not in error: '{error}'.")
        }
    }
}

async fn create_client_and_faulty_server<T>(port: u16, body: T) -> ComponentAClient
where
    T: Serialize + Send + Sync + 'static + Clone,
{
    task::spawn(async move {
        async fn handler<T: Serialize>(
            _http_request: Request<Body>,
            body: T,
        ) -> Result<Response<Body>, hyper::Error> {
            Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(serialize(&body).unwrap()))
                .unwrap())
        }

        let socket = SocketAddr::new(LOCAL_IP, port);
        let make_svc = make_service_fn(|_conn| {
            let body = body.clone();
            async move { Ok::<_, hyper::Error>(service_fn(move |req| handler(req, body.clone()))) }
        });
        Server::bind(&socket).serve(make_svc).await.unwrap();
    });

    // Todo(uriel): Get rid of this
    // Ensure the server starts running.
    task::yield_now().await;

    ComponentAClient::new(LOCAL_IP, port, MAX_RETRIES)
}

async fn setup_for_tests(setup_value: ValueB, a_port: u16, b_port: u16) {
    let a_client = ComponentAClient::new(LOCAL_IP, a_port, MAX_RETRIES);
    let b_client = ComponentBClient::new(LOCAL_IP, b_port, MAX_RETRIES);

    let component_a = ComponentA::new(Box::new(b_client));
    let component_b = ComponentB::new(setup_value, Box::new(a_client.clone()));

    let mut component_a_server = RemoteComponentServer::<
        ComponentA,
        ComponentARequest,
        ComponentAResponse,
    >::new(component_a, LOCAL_IP, a_port);
    let mut component_b_server = RemoteComponentServer::<
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
    let a_client = ComponentAClient::new(LOCAL_IP, A_PORT_TEST_SETUP, MAX_RETRIES);
    let b_client = ComponentBClient::new(LOCAL_IP, B_PORT_TEST_SETUP, MAX_RETRIES);
    test_a_b_functionality(a_client, b_client, setup_value.into()).await;
}

#[tokio::test]
async fn test_faulty_client_setup() {
    // Todo(uriel): Find a better way to pass expected value to the setup
    // 123 is some arbitrary value, we don't check it anyway.
    setup_for_tests(123, A_PORT_FAULTY_CLIENT, B_PORT_FAULTY_CLIENT).await;

    struct FaultyAClient;

    #[async_trait]
    impl ComponentAClientTrait for FaultyAClient {
        async fn a_get_value(&self) -> ResultA {
            let component_request = ARBITRARY_DATA.to_string();
            let uri: Uri =
                format!("http://[{}]:{}/", LOCAL_IP, A_PORT_FAULTY_CLIENT).parse().unwrap();
            let http_request = Request::post(uri)
                .header(CONTENT_TYPE, APPLICATION_OCTET_STREAM)
                .body(Body::from(serialize(&component_request).unwrap()))
                .unwrap();
            let http_response = Client::new().request(http_request).await.unwrap();
            let status_code = http_response.status();
            let body_bytes = to_bytes(http_response.into_body()).await.unwrap();
            let response: ServerError = deserialize(&body_bytes).unwrap();

            Err(ClientError::ResponseError(status_code, response))
        }
    }
    let faulty_a_client = FaultyAClient;
    let expected_error_contained_keywords =
        [StatusCode::BAD_REQUEST.as_str(), DESERIALIZE_REQ_ERROR_MESSAGE];
    verify_error(faulty_a_client, &expected_error_contained_keywords).await;
}

#[tokio::test]
async fn test_unconnected_server() {
    let client = ComponentAClient::new(LOCAL_IP, UNCONNECTED_SERVER_PORT, MAX_RETRIES);

    let expected_error_contained_keywords = ["Connection refused"];
    verify_error(client, &expected_error_contained_keywords).await;
}

#[rstest]
#[case::request_deserialization_failure(
    create_client_and_faulty_server(
        FAULTY_SERVER_REQ_DESER_PORT,
        ServerError::RequestDeserializationFailure(MOCK_SERVER_ERROR.to_string())
    ).await,
    &[StatusCode::BAD_REQUEST.as_str(),DESERIALIZE_REQ_ERROR_MESSAGE, MOCK_SERVER_ERROR],
)]
#[case::response_deserialization_failure(
    create_client_and_faulty_server(FAULTY_SERVER_RES_DESER_PORT,ARBITRARY_DATA).await,
    &[DESERIALIZE_RES_ERROR_MESSAGE],
)]
#[tokio::test]
async fn test_faulty_server(
    #[case] client: ComponentAClient,
    #[case] expected_error_contained_keywords: &[&str],
) {
    verify_error(client, expected_error_contained_keywords).await;
}

#[tokio::test]
async fn test_retry_request() {
    // Spawn a server that responses with OK every other request.
    task::spawn(async move {
        let should_send_ok = Arc::new(Mutex::new(false));
        async fn handler(
            _http_request: Request<Body>,
            should_send_ok: Arc<Mutex<bool>>,
        ) -> Result<Response<Body>, hyper::Error> {
            let mut should_send_ok = should_send_ok.lock().await;
            let body = ComponentAResponse::AGetValue(VALID_VALUE_A);
            let ret = if *should_send_ok {
                Response::builder()
                    .status(StatusCode::OK)
                    .body(Body::from(serialize(&body).unwrap()))
                    .unwrap()
            } else {
                Response::builder()
                    .status(StatusCode::IM_A_TEAPOT)
                    .body(Body::from(serialize(&body).unwrap()))
                    .unwrap()
            };
            *should_send_ok = !*should_send_ok;

            Ok(ret)
        }

        let socket = SocketAddr::new(LOCAL_IP, RETRY_REQ_PORT);
        let make_svc = make_service_fn(|_conn| {
            let should_send_ok = should_send_ok.clone();
            async move {
                Ok::<_, hyper::Error>(service_fn(move |req| handler(req, should_send_ok.clone())))
            }
        });

        Server::bind(&socket).serve(make_svc).await.unwrap();
    });
    // Todo(uriel): Get rid of this
    // Ensure the server starts running.
    task::yield_now().await;

    // The initial server state is 'false', hence the first attempt returns an error and
    // sets the server state to 'true'. The second attempt (first retry) therefore returns a
    // 'success', while setting the server state to 'false' yet again.
    let a_client_retry = ComponentAClient::new(LOCAL_IP, RETRY_REQ_PORT, 1);
    assert_eq!(a_client_retry.a_get_value().await.unwrap(), VALID_VALUE_A);

    // The current server state is 'false', hence the first and only attempt returns an error.
    let a_client_no_retry = ComponentAClient::new(LOCAL_IP, RETRY_REQ_PORT, 0);
    let expected_error_contained_keywords = [DESERIALIZE_RES_ERROR_MESSAGE];
    verify_error(a_client_no_retry.clone(), &expected_error_contained_keywords).await;
}
