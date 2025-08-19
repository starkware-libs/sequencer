use std::fmt::Debug;
use std::future::ready;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use apollo_infra_utils::run_until::run_until;
use async_trait::async_trait;
use hyper::body::to_bytes;
use hyper::header::CONTENT_TYPE;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Request, Response, Server, StatusCode, Uri};
use metrics::set_default_local_recorder;
use metrics_exporter_prometheus::PrometheusBuilder;
use rstest::rstest;
use serde::de::DeserializeOwned;
use serde::Serialize;
use starknet_types_core::felt::Felt;
use tokio::sync::mpsc::channel;
use tokio::sync::{Mutex, Semaphore};
use tokio::task;

use crate::component_client::{
    ClientError,
    ClientResult,
    LocalComponentClient,
    RemoteClientConfig,
    RemoteComponentClient,
};
use crate::component_definitions::{
    ComponentClient,
    RequestWrapper,
    ServerError,
    APPLICATION_OCTET_STREAM,
    BUSY_PREVIOUS_REQUESTS_MSG,
};
use crate::component_server::{
    ComponentServerStarter,
    LocalComponentServer,
    LocalServerConfig,
    RemoteComponentServer,
};
use crate::serde_utils::SerdeWrapper;
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
    AVAILABLE_PORTS,
    TEST_LOCAL_SERVER_METRICS,
    TEST_REMOTE_CLIENT_METRICS,
    TEST_REMOTE_SERVER_METRICS,
};

type ComponentAClient = RemoteComponentClient<ComponentARequest, ComponentAResponse>;
type ComponentBClient = RemoteComponentClient<ComponentBRequest, ComponentBResponse>;

const MOCK_SERVER_ERROR: &str = "mock server error";
const ARBITRARY_DATA: &str = "arbitrary data";
// ServerError::RequestDeserializationFailure error message.
const DESERIALIZE_REQ_ERROR_MESSAGE: &str = "Could not deserialize client request";
const BAD_REQUEST_ERROR_MESSAGE: &str = "Got status code: 400 Bad Request";
const VALID_VALUE_A: ValueA = Felt::ONE;
const MAX_CONCURRENCY: usize = 10;
const FAST_FAILING_CLIENT_CONFIG: RemoteClientConfig = RemoteClientConfig {
    retries: 0,
    idle_connections: 0,
    idle_timeout_ms: 0,
    max_retry_interval_ms: 0,
    initial_retry_delay_ms: 0,
    log_attempt_interval_ms: 1,
};

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
    let socket = AVAILABLE_PORTS.lock().await.get_next_local_host_socket();
    task::spawn(async move {
        async fn handler<T>(
            _http_request: Request<Body>,
            body: T,
        ) -> Result<Response<Body>, hyper::Error>
        where
            T: Serialize + DeserializeOwned + Debug,
        {
            Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(SerdeWrapper::new(body).wrapper_serialize().unwrap()))
                .unwrap())
        }

        let make_svc = make_service_fn(|_conn| {
            let body = body.clone();
            async move { Ok::<_, hyper::Error>(service_fn(move |req| handler(req, body.clone()))) }
        });
        Server::bind(&socket).serve(make_svc).await.unwrap();
    });

    // Ensure the server starts running.
    task::yield_now().await;

    ComponentAClient::new(
        FAST_FAILING_CLIENT_CONFIG,
        &socket.ip().to_string(),
        socket.port(),
        &TEST_REMOTE_CLIENT_METRICS,
    )
}

/// Ensures the remote client respects the server’s concurrency cap:
/// - Two in-flight requests exhaust the limit and a third is **immediately rejected** with `503
///   Service Unavailable` and the message `"Server is busy addressing previous requests"`.
/// - After releasing permits on the shared semaphore, a subsequent request succeeds.
/// This test also verifies that the number of connections to the remote server metric is updated
/// correctly.
#[tokio::test]
async fn remote_connection_concurrency() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = set_default_local_recorder(&recorder);
    TEST_REMOTE_SERVER_METRICS.register();
    const METRIC_SAMPLING_INTERVAL_MILLIS: u64 = 5;
    const MAX_ATTEMPTS: usize = 50;

    let setup_value: ValueB = Felt::from(90);
    let a_socket = AVAILABLE_PORTS.lock().await.get_next_local_host_socket();
    let b_socket = AVAILABLE_PORTS.lock().await.get_next_local_host_socket();

    // Shared semaphore used inside ComponentA::handle_request
    let semaphore = Arc::new(Semaphore::new(0));

    // pass the semaphore into the server’s ComponentA
    setup_for_tests(setup_value, a_socket, b_socket, 2, Some(semaphore.clone())).await;

    let client1 = RemoteComponentClient::<ComponentARequest, ComponentAResponse>::new(
        FAST_FAILING_CLIENT_CONFIG,
        &Ipv4Addr::LOCALHOST.to_string(),
        a_socket.port(),
        &TEST_REMOTE_CLIENT_METRICS,
    );
    let client2 = client1.clone();
    let client3 = client1.clone();
    let client4 = client1.clone();

    // First two requests will block on the semaphore inside ComponentA
    let fut1 =
        tokio::spawn(async move { client1.send(ComponentARequest::AGetValue).await.unwrap() });

    tokio::task::yield_now().await;
    run_until(
        METRIC_SAMPLING_INTERVAL_MILLIS,
        MAX_ATTEMPTS,
        || {
            let metric_recorder = recorder.handle().render();
            ready(
                TEST_REMOTE_SERVER_METRICS
                    .get_number_of_connections_value(metric_recorder.as_str()),
            )
        },
        |connections| *connections == 1,
        None,
    )
    .await
    .unwrap();

    let _fut2 =
        tokio::spawn(async move { client2.send(ComponentARequest::AGetValue).await.unwrap() });

    tokio::task::yield_now().await;
    run_until(
        METRIC_SAMPLING_INTERVAL_MILLIS,
        MAX_ATTEMPTS,
        || {
            let metric_recorder = recorder.handle().render();
            ready(
                TEST_REMOTE_SERVER_METRICS
                    .get_number_of_connections_value(metric_recorder.as_str()),
            )
        },
        |connections| *connections == 2,
        None,
    )
    .await
    .unwrap();

    let err = client3
        .send(ComponentARequest::AGetValue)
        .await
        .expect_err("third request should be rejected when max concurrency is reached");

    match err {
        ClientError::ResponseError(status, server_err) => {
            assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
            let msg = server_err.to_string();
            assert!(
                msg.contains(BUSY_PREVIOUS_REQUESTS_MSG),
                "unexpected server error body: {msg:?}"
            );
        }
        other => panic!("expected ResponseError(503, _), got: {other:?}"),
    }

    run_until(
        METRIC_SAMPLING_INTERVAL_MILLIS,
        MAX_ATTEMPTS,
        || {
            let metric_recorder = recorder.handle().render();
            ready(
                TEST_REMOTE_SERVER_METRICS
                    .get_number_of_connections_value(metric_recorder.as_str()),
            )
        },
        |connections| *connections == 2,
        None,
    )
    .await
    .unwrap();

    // Release one slot
    semaphore.add_permits(1);
    fut1.await.unwrap();

    run_until(
        METRIC_SAMPLING_INTERVAL_MILLIS,
        MAX_ATTEMPTS,
        || {
            let metric_recorder = recorder.handle().render();
            ready(
                TEST_REMOTE_SERVER_METRICS
                    .get_number_of_connections_value(metric_recorder.as_str()),
            )
        },
        |connections| *connections == 1,
        None,
    )
    .await
    .unwrap();

    let result4 = client4.send(ComponentARequest::AGetValue).await;
    assert!(result4.is_ok(), "Expected client4 to succeed after one slot was freed");

    run_until(
        METRIC_SAMPLING_INTERVAL_MILLIS,
        MAX_ATTEMPTS,
        || {
            let metric_recorder = recorder.handle().render();
            ready(
                TEST_REMOTE_SERVER_METRICS
                    .get_number_of_connections_value(metric_recorder.as_str()),
            )
        },
        |connections| *connections == 2,
        None,
    )
    .await
    .unwrap();
}

async fn setup_for_tests(
    setup_value: ValueB,
    a_socket: SocketAddr,
    b_socket: SocketAddr,
    max_concurrency: usize,
    sem: Option<Arc<Semaphore>>,
) {
    let a_config = RemoteClientConfig::default();
    let b_config = RemoteClientConfig::default();

    let a_remote_client = ComponentAClient::new(
        a_config,
        &a_socket.ip().to_string(),
        a_socket.port(),
        &TEST_REMOTE_CLIENT_METRICS,
    );
    let b_remote_client = ComponentBClient::new(
        b_config,
        &b_socket.ip().to_string(),
        b_socket.port(),
        &TEST_REMOTE_CLIENT_METRICS,
    );

    let component_a = match sem {
        Some(s) => ComponentA::with_semaphore(Box::new(b_remote_client), s),
        None => ComponentA::new(Box::new(b_remote_client)),
    };

    let component_b = ComponentB::new(setup_value, Box::new(a_remote_client.clone()));

    let (tx_a, rx_a) = channel::<RequestWrapper<ComponentARequest, ComponentAResponse>>(32);
    let (tx_b, rx_b) = channel::<RequestWrapper<ComponentBRequest, ComponentBResponse>>(32);

    let a_local_client = LocalComponentClient::<ComponentARequest, ComponentAResponse>::new(tx_a);
    let b_local_client = LocalComponentClient::<ComponentBRequest, ComponentBResponse>::new(tx_b);

    let config = LocalServerConfig::default();
    let mut component_a_local_server =
        LocalComponentServer::new(component_a, &config, rx_a, &TEST_LOCAL_SERVER_METRICS);
    let mut component_b_local_server =
        LocalComponentServer::new(component_b, &config, rx_b, &TEST_LOCAL_SERVER_METRICS);

    let mut component_a_remote_server = RemoteComponentServer::new(
        a_local_client,
        a_socket.ip(),
        a_socket.port(),
        max_concurrency,
        TEST_REMOTE_SERVER_METRICS,
    );
    let mut component_b_remote_server = RemoteComponentServer::new(
        b_local_client,
        b_socket.ip(),
        b_socket.port(),
        max_concurrency,
        TEST_REMOTE_SERVER_METRICS,
    );

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
async fn proper_setup() {
    let setup_value: ValueB = Felt::from(90);
    let a_socket = AVAILABLE_PORTS.lock().await.get_next_local_host_socket();
    let b_socket = AVAILABLE_PORTS.lock().await.get_next_local_host_socket();

    setup_for_tests(setup_value, a_socket, b_socket, MAX_CONCURRENCY, None).await;
    let a_client_config = RemoteClientConfig::default();
    let b_client_config = RemoteClientConfig::default();

    let a_remote_client = ComponentAClient::new(
        a_client_config,
        &a_socket.ip().to_string(),
        a_socket.port(),
        &TEST_REMOTE_CLIENT_METRICS,
    );
    let b_remote_client = ComponentBClient::new(
        b_client_config,
        &b_socket.ip().to_string(),
        b_socket.port(),
        &TEST_REMOTE_CLIENT_METRICS,
    );

    test_a_b_functionality(a_remote_client, b_remote_client, setup_value).await;
}

#[tokio::test]
async fn faulty_client_setup() {
    let a_socket = AVAILABLE_PORTS.lock().await.get_next_local_host_socket();
    let b_socket = AVAILABLE_PORTS.lock().await.get_next_local_host_socket();
    // Todo(uriel): Find a better way to pass expected value to the setup
    // 123 is some arbitrary value, we don't check it anyway.
    setup_for_tests(Felt::from(123), a_socket, b_socket, MAX_CONCURRENCY, None).await;

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
                .body(Body::from(SerdeWrapper::new(component_request).wrapper_serialize().unwrap()))
                .unwrap();
            let http_response = Client::new().request(http_request).await.unwrap();
            let status_code = http_response.status();
            let body_bytes = to_bytes(http_response.into_body()).await.unwrap();
            let response = SerdeWrapper::<ServerError>::wrapper_deserialize(&body_bytes).unwrap();
            Err(ClientError::ResponseError(status_code, response))
        }
    }
    let faulty_a_client = FaultyAClient { socket: a_socket };
    let expected_error_contained_keywords =
        [StatusCode::BAD_REQUEST.as_str(), DESERIALIZE_REQ_ERROR_MESSAGE];
    verify_error(faulty_a_client, &expected_error_contained_keywords).await;
}

#[tokio::test]
async fn unconnected_server() {
    let socket = AVAILABLE_PORTS.lock().await.get_next_local_host_socket();
    let client = ComponentAClient::new(
        FAST_FAILING_CLIENT_CONFIG,
        &socket.ip().to_string(),
        socket.port(),
        &TEST_REMOTE_CLIENT_METRICS,
    );
    let expected_error_contained_keywords = ["Connection refused"];
    verify_error(client, &expected_error_contained_keywords).await;
}

// TODO(Nadin): add DESERIALIZE_REQ_ERROR_MESSAGE to the expected error keywords in the first case.
#[rstest]
#[case::request_deserialization_failure(
    create_client_and_faulty_server(
        ServerError::RequestDeserializationFailure(MOCK_SERVER_ERROR.to_string())
    ).await,
    &[StatusCode::BAD_REQUEST.as_str()],
)]
#[case::response_deserialization_failure(
    create_client_and_faulty_server(ARBITRARY_DATA.to_string()).await,
    &[BAD_REQUEST_ERROR_MESSAGE],
)]
#[tokio::test]
async fn faulty_server(
    #[case] client: ComponentAClient,
    #[case] expected_error_contained_keywords: &[&str],
) {
    verify_error(client, expected_error_contained_keywords).await;
}

#[tokio::test]
async fn retry_request() {
    let socket = AVAILABLE_PORTS.lock().await.get_next_local_host_socket();
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
                    .body(Body::from(SerdeWrapper::new(body).wrapper_serialize().unwrap()))
                    .unwrap()
            } else {
                Response::builder()
                    .status(StatusCode::IM_A_TEAPOT)
                    .body(Body::from(SerdeWrapper::new(body).wrapper_serialize().unwrap()))
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
    let retry_config = RemoteClientConfig { retries: 1, ..Default::default() };
    let a_client_retry = ComponentAClient::new(
        retry_config,
        &socket.ip().to_string(),
        socket.port(),
        &TEST_REMOTE_CLIENT_METRICS,
    );
    assert_eq!(a_client_retry.a_get_value().await.unwrap(), VALID_VALUE_A);

    // The current server state is 'false', hence the first and only attempt returns an error.
    let no_retry_config = RemoteClientConfig { retries: 0, ..Default::default() };
    let a_client_no_retry = ComponentAClient::new(
        no_retry_config,
        &socket.ip().to_string(),
        socket.port(),
        &TEST_REMOTE_CLIENT_METRICS,
    );
    let expected_error_contained_keywords = [StatusCode::IM_A_TEAPOT.as_str()];
    verify_error(a_client_no_retry.clone(), &expected_error_contained_keywords).await;
}
