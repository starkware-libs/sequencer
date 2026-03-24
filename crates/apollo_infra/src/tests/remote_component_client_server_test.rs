use std::convert::Infallible;
use std::fmt::Debug;
use std::future::ready;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use apollo_infra_utils::run_until::run_until;
use apollo_proc_macros::unique_u16;
use async_trait::async_trait;
use bytes::Bytes;
use http::header::CONTENT_TYPE;
use http::{StatusCode, Uri};
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as Http2ServerBuilder;
use metrics::set_default_local_recorder;
use metrics_exporter_prometheus::PrometheusBuilder;
use rstest::rstest;
use serde::de::DeserializeOwned;
use serde::Serialize;
use socket2::{SockRef, TcpKeepalive};
use starknet_types_core::felt::Felt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::channel;
use tokio::sync::{Mutex, Semaphore};
use tokio::task;
use tokio::time::{sleep, timeout};

use crate::component_client::{
    ClientError,
    ClientResult,
    LocalComponentClient,
    RemoteClientConfig,
    RemoteComponentClient,
    REQUEST_TIMEOUT_ERROR_MESSAGE,
};
use crate::component_definitions::{
    ComponentClient,
    RequestId,
    RequestWrapper,
    ServerError,
    APPLICATION_OCTET_STREAM,
    BUSY_PREVIOUS_REQUESTS_MSG,
    REQUEST_ID_HEADER,
    TCP_KEEPALIVE_FACTOR,
};
use crate::component_server::{
    ComponentServerStarter,
    LocalComponentServer,
    LocalServerConfig,
    RemoteComponentServer,
    RemoteServerConfig,
};
use crate::serde_utils::SerdeWrapper;
use crate::tests::test_utils::client_socket_keepalive_time;
use crate::tests::{
    available_ports_factory,
    dummy_remote_server_config,
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
    keepalive_timeout_ms: 0,
    max_retry_interval_ms: 0,
    initial_retry_delay_ms: 0,
    attempts_per_log: 1,
    connection_timeout_ms: 500,
    request_timeout_ms: 1000,
    set_tcp_nodelay: true,
    max_response_body_bytes: usize::MAX,
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

async fn create_client_and_faulty_server<T>(index: u16, body: T) -> ComponentAClient
where
    T: Serialize + DeserializeOwned + Debug + Send + Sync + 'static + Clone,
{
    let socket = available_ports_factory(index).get_next_local_host_socket();
    task::spawn(async move {
        async fn handler<T>(
            _http_request: Request<Incoming>,
            body: T,
        ) -> Result<Response<Full<Bytes>>, Infallible>
        where
            T: Serialize + DeserializeOwned + Debug,
        {
            Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Full::new(Bytes::from(SerdeWrapper::new(body).wrapper_serialize().unwrap())))
                .unwrap())
        }

        let listener = TcpListener::bind(&socket).await.unwrap();
        let (stream, _) = listener.accept().await.unwrap();
        let io = TokioIo::new(stream);
        let service = service_fn(move |req| {
            let body = body.clone();
            async move { handler(req, body).await }
        });
        let _ = Http2ServerBuilder::new(TokioExecutor::new())
            .http2()
            .serve_connection(io, service)
            .await;
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
    let mut available_ports = available_ports_factory(unique_u16!());
    let a_socket = available_ports.get_next_local_host_socket();
    let b_socket = available_ports.get_next_local_host_socket();

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

    let a_local_client = LocalComponentClient::<ComponentARequest, ComponentAResponse>::new(
        tx_a,
        &TEST_LOCAL_CLIENT_METRICS,
    );
    let b_local_client = LocalComponentClient::<ComponentBRequest, ComponentBResponse>::new(
        tx_b,
        &TEST_LOCAL_CLIENT_METRICS,
    );

    let config = LocalServerConfig { max_concurrency, ..Default::default() };
    let mut component_a_local_server =
        LocalComponentServer::new(component_a, &config, rx_a, &TEST_LOCAL_SERVER_METRICS);
    let mut component_b_local_server =
        LocalComponentServer::new(component_b, &config, rx_b, &TEST_LOCAL_SERVER_METRICS);

    let mut component_a_remote_server = RemoteComponentServer::new(
        a_local_client,
        dummy_remote_server_config(a_socket.ip(), max_concurrency),
        a_socket.port(),
        &TEST_REMOTE_SERVER_METRICS,
    );
    let mut component_b_remote_server = RemoteComponentServer::new(
        b_local_client,
        dummy_remote_server_config(b_socket.ip(), max_concurrency),
        b_socket.port(),
        &TEST_REMOTE_SERVER_METRICS,
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
    let mut available_ports = available_ports_factory(unique_u16!());
    let a_socket = available_ports.get_next_local_host_socket();
    let b_socket = available_ports.get_next_local_host_socket();

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
    let mut available_ports = available_ports_factory(unique_u16!());
    let a_socket = available_ports.get_next_local_host_socket();
    let b_socket = available_ports.get_next_local_host_socket();
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
                .header(REQUEST_ID_HEADER, RequestId::generate().to_string())
                .body(Full::new(Bytes::from(
                    SerdeWrapper::new(component_request).wrapper_serialize().unwrap(),
                )))
                .unwrap();
            let http_response = Client::builder(TokioExecutor::new())
                .build_http()
                .request(http_request)
                .await
                .unwrap();
            let status_code = http_response.status();
            let body_bytes = http_response.into_body().collect().await.unwrap().to_bytes();
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
    let socket = available_ports_factory(unique_u16!()).get_next_local_host_socket();
    let client = ComponentAClient::new(
        FAST_FAILING_CLIENT_CONFIG,
        &socket.ip().to_string(),
        socket.port(),
        &TEST_REMOTE_CLIENT_METRICS,
    );
    let expected_error_contained_keywords = ["client error (Connect)"];
    verify_error(client, &expected_error_contained_keywords).await;
}

/// Server that accepts the connection and request but never sends a response (handler never
/// completes). Verifies the client times out and returns
/// CommunicationFailure(REQUEST_TIMEOUT_ERROR_MESSAGE).
#[tokio::test]
async fn request_times_out_when_server_never_responds() {
    let socket = available_ports_factory(unique_u16!()).get_next_local_host_socket();
    task::spawn(async move {
        async fn handler_that_never_responds(
            _http_request: Request<Incoming>,
        ) -> Result<Response<Full<Bytes>>, Infallible> {
            std::future::pending().await
        }

        let listener = TcpListener::bind(&socket).await.unwrap();
        let (stream, _) = listener.accept().await.unwrap();
        let io = TokioIo::new(stream);
        let service = service_fn(|req| async move { handler_that_never_responds(req).await });
        let _ = Http2ServerBuilder::new(TokioExecutor::new())
            .http2()
            .serve_connection(io, service)
            .await;
    });

    task::yield_now().await;

    let timeout_config =
        RemoteClientConfig { request_timeout_ms: 200, retries: 0, ..FAST_FAILING_CLIENT_CONFIG };
    let client = ComponentAClient::new(
        timeout_config,
        &socket.ip().to_string(),
        socket.port(),
        &TEST_REMOTE_CLIENT_METRICS,
    );
    let expected_error_contained_keywords = [REQUEST_TIMEOUT_ERROR_MESSAGE];
    verify_error(client, &expected_error_contained_keywords).await;
}

// TODO(Nadin): add DESERIALIZE_REQ_ERROR_MESSAGE to the expected error keywords in the first case.
#[rstest]
#[case::request_deserialization_failure(
    create_client_and_faulty_server(unique_u16!(),
        ServerError::RequestDeserializationFailure(MOCK_SERVER_ERROR.to_string())
    ).await,
    &[StatusCode::BAD_REQUEST.as_str()],
)]
#[case::response_deserialization_failure(
    create_client_and_faulty_server(unique_u16!(), ARBITRARY_DATA.to_string()).await,
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
    let socket = available_ports_factory(unique_u16!()).get_next_local_host_socket();
    // Spawn a server that responses with OK every other request.
    task::spawn(async move {
        let should_send_ok = Arc::new(Mutex::new(false));
        async fn handler(
            _http_request: Request<Incoming>,
            should_send_ok: Arc<Mutex<bool>>,
        ) -> Result<Response<Full<Bytes>>, Infallible> {
            let mut should_send_ok = should_send_ok.lock().await;
            let body = ComponentAResponse::AGetValue(VALID_VALUE_A);
            let ret = if *should_send_ok {
                Response::builder()
                    .status(StatusCode::OK)
                    .body(Full::new(Bytes::from(
                        SerdeWrapper::new(body).wrapper_serialize().unwrap(),
                    )))
                    .unwrap()
            } else {
                Response::builder()
                    .status(StatusCode::IM_A_TEAPOT)
                    .body(Full::new(Bytes::from(
                        SerdeWrapper::new(body).wrapper_serialize().unwrap(),
                    )))
                    .unwrap()
            };
            *should_send_ok = !*should_send_ok;

            Ok(ret)
        }

        let listener = TcpListener::bind(&socket).await.unwrap();
        loop {
            let Ok((stream, _)) = listener.accept().await else { continue };
            let io = TokioIo::new(stream);
            let should_send_ok = should_send_ok.clone();
            let service = service_fn(move |req| {
                let should_send_ok = should_send_ok.clone();
                async move { handler(req, should_send_ok).await }
            });
            tokio::spawn(async move {
                let _ = Http2ServerBuilder::new(TokioExecutor::new())
                    .http2()
                    .serve_connection(io, service)
                    .await;
            });
        }
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

/// Connects a raw TCP stream to `addr`, performs the HTTP/2 connection preface and SETTINGS
/// exchange, then returns the stream without ever responding to PING frames — simulating a
/// zombie connection.
async fn connect_zombie(addr: SocketAddr) -> TcpStream {
    let mut stream = TcpStream::connect(addr).await.unwrap();

    // HTTP/2 client connection preface.
    stream.write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n").await.unwrap();
    // Empty SETTINGS frame: length=0, type=0x4 (SETTINGS), flags=0x0, stream_id=0.
    stream.write_all(&[0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00]).await.unwrap();

    // Read server frames, accumulating bytes to handle fragmentation. Once we see the
    // server's SETTINGS frame, respond with SETTINGS_ACK and stop — becoming a zombie that
    // ignores all subsequent frames (including PING).
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    'done: loop {
        let n = stream.read(&mut tmp).await.unwrap();
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);

        let mut pos = 0;
        while pos + 9 <= buf.len() {
            let length = (usize::from(buf[pos]) << 16)
                | (usize::from(buf[pos + 1]) << 8)
                | usize::from(buf[pos + 2]);
            if pos + 9 + length > buf.len() {
                break; // incomplete frame, read more
            }
            let frame_type = buf[pos + 3];
            let flags = buf[pos + 4];
            if frame_type == 0x04 /* SETTINGS */ && flags & 0x01 == 0
            // not ACK
            {
                // Send SETTINGS_ACK and stop responding to anything.
                stream
                    .write_all(&[0x00, 0x00, 0x00, 0x04, 0x01, 0x00, 0x00, 0x00, 0x00])
                    .await
                    .unwrap();
                break 'done;
            }
            pos += 9 + length;
        }
    }
    stream
}

/// Verifies that the server closes a zombie connection after the keepalive interval and
/// timeout elapse without receiving a PING response.
#[tokio::test]
async fn zombie_connection_is_evicted() {
    const KEEPALIVE_INTERVAL_MS: u64 = 100;
    const KEEPALIVE_TIMEOUT_MS: u64 = 100;
    const MARGIN_MS: u64 = 500;

    let socket = available_ports_factory(unique_u16!()).get_next_local_host_socket();

    // The local channel is kept alive but never consumed — no requests will be processed.
    let (tx, _rx) = channel::<RequestWrapper<ComponentARequest, ComponentAResponse>>(32);
    let local_client = LocalComponentClient::<ComponentARequest, ComponentAResponse>::new(
        tx,
        &TEST_LOCAL_CLIENT_METRICS,
    );
    let config = RemoteServerConfig {
        keepalive_interval_ms: KEEPALIVE_INTERVAL_MS,
        keepalive_timeout_ms: KEEPALIVE_TIMEOUT_MS,
        ..dummy_remote_server_config(socket.ip(), MAX_CONCURRENCY)
    };
    let mut server = RemoteComponentServer::new(
        local_client,
        config,
        socket.port(),
        &TEST_REMOTE_SERVER_METRICS,
    );
    task::spawn(async move { server.start().await });
    task::yield_now().await;

    let mut zombie = connect_zombie(socket).await;

    // read_to_end blocks until the server closes the connection (GOAWAY + FIN). The timeout
    // covers the full keepalive cycle plus a scheduling margin.
    let mut buf = Vec::new();
    let read_result = timeout(
        Duration::from_millis(KEEPALIVE_INTERVAL_MS + KEEPALIVE_TIMEOUT_MS + MARGIN_MS),
        zombie.read_to_end(&mut buf),
    )
    .await;
    assert!(
        read_result.is_ok(),
        "Server should have closed the zombie connection after keepalive timeout, but the \
         connection is still open"
    );
}

/// Verifies that Hyper evicts an idle connection from its pool after `http_pool_idle_timeout_ms`
/// and opens a fresh TCP connection for the next request.
///
/// Since no pool timer is configured, eviction is triggered by the next pool checkout (the second
/// request), not a background task. A new connection is detected by counting server-side accepts.
#[tokio::test]
async fn idle_connection_is_evicted_after_pool_timeout() {
    const KEEPALIVE_TIMEOUT_MS: u64 = 100;
    const MARGIN_MS: u64 = 300;

    let socket = available_ports_factory(unique_u16!()).get_next_local_host_socket();
    let accept_count = Arc::new(AtomicUsize::new(0));

    {
        let accept_count = accept_count.clone();
        task::spawn(async move {
            let listener = TcpListener::bind(socket).await.unwrap();
            loop {
                let Ok((stream, _)) = listener.accept().await else { continue };
                accept_count.fetch_add(1, Ordering::SeqCst);
                let io = TokioIo::new(stream);
                let service = service_fn(|_req: Request<Incoming>| async {
                    let body = ComponentAResponse::AGetValue(VALID_VALUE_A);
                    Ok::<_, Infallible>(
                        Response::builder()
                            .status(StatusCode::OK)
                            .header(CONTENT_TYPE, APPLICATION_OCTET_STREAM)
                            .body(Full::new(Bytes::from(
                                SerdeWrapper::new(body).wrapper_serialize().unwrap(),
                            )))
                            .unwrap(),
                    )
                });
                tokio::spawn(async move {
                    let _ = Http2ServerBuilder::new(TokioExecutor::new())
                        .http2()
                        .serve_connection(io, service)
                        .await;
                });
            }
        });
    }
    task::yield_now().await;

    let client = ComponentAClient::new(
        RemoteClientConfig { keepalive_timeout_ms: KEEPALIVE_TIMEOUT_MS, ..Default::default() },
        &socket.ip().to_string(),
        socket.port(),
        &TEST_REMOTE_CLIENT_METRICS,
    );

    // Establish connection C1.
    client.a_get_value().await.expect("first request should succeed");
    assert_eq!(accept_count.load(Ordering::SeqCst), 1, "C1 should have been accepted");

    // Let the idle timeout expire.
    sleep(Duration::from_millis(KEEPALIVE_TIMEOUT_MS + MARGIN_MS)).await;

    // The next checkout detects C1 is expired, drops it, and opens a new connection C2.
    client.a_get_value().await.expect("second request should succeed");
    assert_eq!(
        accept_count.load(Ordering::SeqCst),
        2,
        "idle timeout should have evicted C1 and caused a new connection C2"
    );
}

/// Verifies that `SO_KEEPALIVE` on a server-accepted socket reflects `idle_time_ms`.
///
/// The test accepts the connection itself so it owns the `TcpStream` and can inspect socket
/// options via `SockRef::from` without any unsafe FD scanning.
#[rstest]
#[tokio::test]
async fn server_tcp_keepalive_socket_option_matches_config() {
    // Linux TCP_KEEPIDLE has 1-second granularity; values below 1000ms round to 0 and fail.
    const ARBITRARY_IDLE_TIMEOUT_MS: u64 = 1000;

    let listener =
        TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)).await.unwrap();
    let server_addr = listener.local_addr().unwrap();

    let _client_stream = TcpStream::connect(server_addr).await.unwrap();
    let (accepted_stream, _) = listener.accept().await.unwrap();

    // Mirror the keepalive logic in RemoteComponentServer::start().
    let keepalive = TcpKeepalive::new().with_time(Duration::from_millis(ARBITRARY_IDLE_TIMEOUT_MS));
    SockRef::from(&accepted_stream).set_tcp_keepalive(&keepalive).unwrap();

    assert!(
        SockRef::from(&accepted_stream).keepalive().unwrap(),
        "SO_KEEPALIVE on the accepted socket should reflect idle_time_ms"
    );
}

/// Verifies that a `RemoteComponentServer` configured with TCP keepalive correctly evicts
/// zombie connections.
///
/// On loopback the OS always acknowledges TCP keepalive probes, so the connection is evicted
/// via the HTTP/2 PING mechanism rather than TCP keepalive itself. The test verifies that
/// enabling TCP keepalive does not interfere with connection eviction.
#[tokio::test]
async fn server_with_tcp_keepalive_evicts_zombie_connection() {
    const KEEPALIVE_INTERVAL_MS: u64 = 100;
    const KEEPALIVE_TIMEOUT_MS: u64 = 100;
    const MARGIN_MS: u64 = 500;

    let socket = available_ports_factory(unique_u16!()).get_next_local_host_socket();

    let (tx, _rx) = channel::<RequestWrapper<ComponentARequest, ComponentAResponse>>(32);
    let local_client = LocalComponentClient::<ComponentARequest, ComponentAResponse>::new(
        tx,
        &TEST_LOCAL_CLIENT_METRICS,
    );
    let config = RemoteServerConfig {
        keepalive_interval_ms: KEEPALIVE_INTERVAL_MS,
        keepalive_timeout_ms: KEEPALIVE_TIMEOUT_MS,
        ..dummy_remote_server_config(socket.ip(), MAX_CONCURRENCY)
    };
    let mut server = RemoteComponentServer::new(
        local_client,
        config,
        socket.port(),
        &TEST_REMOTE_SERVER_METRICS,
    );
    task::spawn(async move { server.start().await });
    task::yield_now().await;

    let mut zombie = connect_zombie(socket).await;

    let mut buf = Vec::new();
    let read_result = timeout(
        Duration::from_millis(KEEPALIVE_INTERVAL_MS + KEEPALIVE_TIMEOUT_MS + MARGIN_MS),
        zombie.read_to_end(&mut buf),
    )
    .await;
    assert!(
        read_result.is_ok(),
        "Server should have closed the zombie connection after keepalive timeout, but the \
         connection is still open"
    );
}

/// Verifies that `TCP_KEEPIDLE` on the client's outbound socket equals
/// `keepalive_timeout_ms * TCP_KEEPALIVE_FACTOR`, confirming the socket is armed to probe after
/// exactly the expected idle period.
///
/// Internally, hyper's `HttpConnector::set_keepalive` calls `SockRef::set_tcp_keepalive` via
/// socket2 only when it establishes a TCP connection for a request. The socket therefore only
/// exists — and the option is only applied — after the first `send`. The test triggers that path,
/// then reads the option back through the raw file-descriptor scan to confirm config value was
/// properly applied on the socket.
///
/// Note: an end-to-end test that the OS closes the socket after unanswered probes would require
/// packet-level manipulation (e.g. iptables DROP on loopback) and is out of scope for unit tests.
#[tokio::test]
async fn tcp_keepalive_idle_time_matches_config() {
    // 2000 * 1.5 = 3000 ms = 3 s exactly; socket2 stores TCP_KEEPIDLE in whole seconds, so the
    // configured duration must be a whole number of seconds or the comparison fails.
    const KEEPALIVE_TIMEOUT_MS: u64 = 2000;
    let expected_keepalive_idle =
        Duration::from_millis(KEEPALIVE_TIMEOUT_MS).mul_f64(TCP_KEEPALIVE_FACTOR);
    assert_eq!(
        expected_keepalive_idle.subsec_nanos(),
        0,
        "KEEPALIVE_TIMEOUT_MS * TCP_KEEPALIVE_FACTOR must be a whole number of seconds"
    );

    let mut ports = available_ports_factory(unique_u16!());
    let a_socket = ports.get_next_local_host_socket();
    let b_socket = ports.get_next_local_host_socket();

    setup_for_tests(VALID_VALUE_A, a_socket, b_socket, MAX_CONCURRENCY, None).await;

    let client = ComponentAClient::new(
        RemoteClientConfig { keepalive_timeout_ms: KEEPALIVE_TIMEOUT_MS, ..Default::default() },
        &a_socket.ip().to_string(),
        a_socket.port(),
        &TEST_REMOTE_CLIENT_METRICS,
    );

    // Trigger the lazy TCP connect so the socket exists and keepalive options are applied.
    client.a_get_value().await.expect("request should succeed");

    let actual_keepalive_idle = client_socket_keepalive_time(a_socket)
        .expect("SO_KEEPALIVE should be set and TCP_KEEPIDLE should be readable");
    assert_eq!(
        actual_keepalive_idle, expected_keepalive_idle,
        "TCP_KEEPIDLE should equal keepalive_timeout_ms * TCP_KEEPALIVE_FACTOR"
    );
}
