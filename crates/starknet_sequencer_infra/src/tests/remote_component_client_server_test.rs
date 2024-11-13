use std::fmt::Debug;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use hyper::body::to_bytes;
use hyper::header::CONTENT_TYPE;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Request, Response, Server, StatusCode, Uri};
use rstest::rstest;
use serde::de::DeserializeOwned;
use serde::Serialize;
use starknet_types_core::felt::Felt;
use tokio::sync::mpsc::channel;
use tokio::sync::Mutex;
use tokio::task;

use crate::component_client::{
    ClientError,
    ClientResult,
    LocalComponentClient,
    RemoteComponentClient,
};
use crate::component_definitions::{
    ComponentRequestAndResponseSender,
    RemoteClientConfig,
    RemoteServerConfig,
    ServerError,
    APPLICATION_OCTET_STREAM,
};
use crate::component_server::{
    ComponentServerStarter,
    LocalComponentServer,
    RemoteComponentServer,
};
use crate::serde_utils::BincodeSerdeWrapper;
use crate::test_utils::get_available_socket;
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
};

type ComponentAClient = RemoteComponentClient<ComponentARequest, ComponentAResponse>;
type ComponentBClient = RemoteComponentClient<ComponentBRequest, ComponentBResponse>;

const MAX_IDLE_CONNECTION: usize = usize::MAX;
const IDLE_TIMEOUT: u64 = 90;
const MOCK_SERVER_ERROR: &str = "mock server error";
const ARBITRARY_DATA: &str = "arbitrary data";
// ServerError::RequestDeserializationFailure error message.
const DESERIALIZE_REQ_ERROR_MESSAGE: &str = "Could not deserialize client request";
// ClientError::ResponseDeserializationFailure error message.
const DESERIALIZE_RES_ERROR_MESSAGE: &str = "Could not deserialize server response";
const VALID_VALUE_A: ValueA = Felt::ONE;

#[async_trait]
impl ComponentAClientTrait for RemoteComponentClient<ComponentARequest, ComponentAResponse> {
    async fn a_get_value(&self) -> ResultA {
        match self.send(ComponentARequest::AGetValue).await? {
            ComponentAResponse::AGetValue(value) => Ok(value),
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

async fn verify_error(
    a_remote_client: impl ComponentAClientTrait,
    expected_error_contained_keywords: &[&str],
) {
    let Err(error) = a_remote_client.a_get_value().await else {
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

async fn create_client_and_faulty_server<T>(body: T) -> ComponentAClient
where
    T: Serialize + DeserializeOwned + Debug + Send + Sync + 'static + Clone,
{
    let socket = get_available_socket().await;
    task::spawn(async move {
        async fn handler<T>(
            _http_request: Request<Body>,
            body: T,
        ) -> Result<Response<Body>, hyper::Error>
        where
            T: Serialize + DeserializeOwned + Debug + Send + Sync + Clone,
        {
            Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(BincodeSerdeWrapper::new(body).to_bincode().unwrap()))
                .unwrap())
        }

        let make_svc = make_service_fn(|_conn| {
            let body = body.clone();
            async move { Ok::<_, hyper::Error>(service_fn(move |req| handler(req, body.clone()))) }
        });
        Server::bind(&socket).serve(make_svc).await.unwrap();
    });

    // Todo(uriel): Get rid of this
    // Ensure the server starts running.
    task::yield_now().await;

    let config = RemoteClientConfig { socket, ..Default::default() };
    ComponentAClient::new(config)
}

async fn setup_for_tests(setup_value: ValueB, a_socket: SocketAddr, b_socket: SocketAddr) {
    let a_config = RemoteClientConfig { socket: a_socket, ..Default::default() };
    let b_config = RemoteClientConfig { socket: b_socket, ..Default::default() };

    let a_remote_client = ComponentAClient::new(a_config);
    let b_remote_client = ComponentBClient::new(b_config);

    let component_a = ComponentA::new(Box::new(b_remote_client));
    let component_b = ComponentB::new(setup_value, Box::new(a_remote_client.clone()));

    let (tx_a, rx_a) =
        channel::<ComponentRequestAndResponseSender<ComponentARequest, ComponentAResponse>>(32);
    let (tx_b, rx_b) =
        channel::<ComponentRequestAndResponseSender<ComponentBRequest, ComponentBResponse>>(32);

    let a_local_client = LocalComponentClient::<ComponentARequest, ComponentAResponse>::new(tx_a);
    let b_local_client = LocalComponentClient::<ComponentBRequest, ComponentBResponse>::new(tx_b);

    let mut component_a_local_server = LocalComponentServer::new(component_a, rx_a);
    let mut component_b_local_server = LocalComponentServer::new(component_b, rx_b);

    let mut component_a_remote_server =
        RemoteComponentServer::new(a_local_client, RemoteServerConfig { socket: a_socket });
    let mut component_b_remote_server =
        RemoteComponentServer::new(b_local_client, RemoteServerConfig { socket: b_socket });

    task::spawn(async move {
        let _ = component_a_local_server.start().await;
    });
    task::spawn(async move {
        let _ = component_b_local_server.start().await;
    });

    task::spawn(async move {
        let _ = component_a_remote_server.start().await;
    });

    task::spawn(async move {
        let _ = component_b_remote_server.start().await;
    });

    // Todo(uriel): Get rid of this
    task::yield_now().await;
}

#[tokio::test]
async fn test_proper_setup() {
    let setup_value: ValueB = Felt::from(90);
    let a_socket = get_available_socket().await;
    let b_socket = get_available_socket().await;

    setup_for_tests(setup_value, a_socket, b_socket).await;
    let a_client_config = RemoteClientConfig { socket: a_socket, ..Default::default() };
    let b_client_config = RemoteClientConfig { socket: b_socket, ..Default::default() };

    let a_remote_client = ComponentAClient::new(a_client_config);
    let b_remote_client = ComponentBClient::new(b_client_config);
    test_a_b_functionality(a_remote_client, b_remote_client, setup_value).await;
}

#[tokio::test]
async fn test_faulty_client_setup() {
    let a_socket = get_available_socket().await;
    let b_socket = get_available_socket().await;
    // Todo(uriel): Find a better way to pass expected value to the setup
    // 123 is some arbitrary value, we don't check it anyway.
    setup_for_tests(Felt::from(123), a_socket, b_socket).await;

    struct FaultyAClient {
        socket: SocketAddr,
    }

    #[async_trait]
    impl ComponentAClientTrait for FaultyAClient {
        async fn a_get_value(&self) -> ResultA {
            let component_request = ARBITRARY_DATA.to_string();
            let uri: Uri =
                format!("http://[{}]:{}/", self.socket.ip(), self.socket.port()).parse().unwrap();
            let http_request = Request::post(uri)
                .header(CONTENT_TYPE, APPLICATION_OCTET_STREAM)
                .body(Body::from(BincodeSerdeWrapper::new(component_request).to_bincode().unwrap()))
                .unwrap();
            let http_response = Client::new().request(http_request).await.unwrap();
            let status_code = http_response.status();
            let body_bytes = to_bytes(http_response.into_body()).await.unwrap();
            let response = BincodeSerdeWrapper::<ServerError>::from_bincode(&body_bytes).unwrap();
            Err(ClientError::ResponseError(status_code, response))
        }
    }
    let faulty_a_client = FaultyAClient { socket: a_socket };
    let expected_error_contained_keywords =
        [StatusCode::BAD_REQUEST.as_str(), DESERIALIZE_REQ_ERROR_MESSAGE];
    verify_error(faulty_a_client, &expected_error_contained_keywords).await;
}

#[tokio::test]
async fn test_unconnected_server() {
    let socket = get_available_socket().await;
    let client_config = RemoteClientConfig { socket, ..Default::default() };
    let client = ComponentAClient::new(client_config);
    let expected_error_contained_keywords = ["Connection refused"];
    verify_error(client, &expected_error_contained_keywords).await;
}

#[rstest]
#[case::request_deserialization_failure(
    create_client_and_faulty_server(
        ServerError::RequestDeserializationFailure(MOCK_SERVER_ERROR.to_string())
    ).await,
    &[StatusCode::BAD_REQUEST.as_str(),DESERIALIZE_REQ_ERROR_MESSAGE, MOCK_SERVER_ERROR],
)]
#[case::response_deserialization_failure(
    create_client_and_faulty_server(ARBITRARY_DATA.to_string()).await,
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
    let socket = get_available_socket().await;
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
                    .body(Body::from(BincodeSerdeWrapper::new(body).to_bincode().unwrap()))
                    .unwrap()
            } else {
                Response::builder()
                    .status(StatusCode::IM_A_TEAPOT)
                    .body(Body::from(BincodeSerdeWrapper::new(body).to_bincode().unwrap()))
                    .unwrap()
            };
            *should_send_ok = !*should_send_ok;

            Ok(ret)
        }

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
    let retry_config = RemoteClientConfig {
        socket,
        retries: 1,
        idle_connections: MAX_IDLE_CONNECTION,
        idle_timeout: IDLE_TIMEOUT,
    };
    let a_client_retry = ComponentAClient::new(retry_config);
    assert_eq!(a_client_retry.a_get_value().await.unwrap(), VALID_VALUE_A);

    // The current server state is 'false', hence the first and only attempt returns an error.
    let no_retry_config = RemoteClientConfig {
        socket,
        retries: 0,
        idle_connections: MAX_IDLE_CONNECTION,
        idle_timeout: IDLE_TIMEOUT,
    };
    let a_client_no_retry = ComponentAClient::new(no_retry_config);
    let expected_error_contained_keywords = [StatusCode::IM_A_TEAPOT.as_str()];
    verify_error(a_client_no_retry.clone(), &expected_error_contained_keywords).await;
}
