use std::convert::Infallible;

use apollo_proc_macros::unique_u16;
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
use tokio::net::TcpListener;
use tokio::sync::mpsc::channel;
use tokio::task;

use crate::component_client::{ClientError, LocalComponentClient, RemoteClientConfig};
use crate::component_definitions::{
    RequestId,
    RequestWrapper,
    ServerError,
    APPLICATION_OCTET_STREAM,
    REQUEST_ID_HEADER,
};
use crate::component_server::{
    ComponentServerStarter,
    LocalComponentServer,
    LocalServerConfig,
    RemoteComponentServer,
    RemoteServerConfig,
};
use crate::serde_utils::SerdeWrapper;
use crate::tests::test_utils::{
    available_ports_factory,
    ComponentA,
    ComponentAClient,
    ComponentAClientTrait,
    ComponentARequest,
    ComponentAResponse,
    ComponentBClient,
    FAST_FAILING_CLIENT_CONFIG,
    TEST_LOCAL_CLIENT_METRICS,
    TEST_LOCAL_SERVER_METRICS,
    TEST_REMOTE_CLIENT_METRICS,
    TEST_REMOTE_SERVER_METRICS,
};

/// Server rejects a request whose body exceeds `max_request_body_bytes` with 413 and
/// `ServerError::RequestBodyTooLarge`.
#[tokio::test]
async fn request_body_too_large() {
    let mut available_ports = available_ports_factory(unique_u16!());
    let a_socket = available_ports.get_next_local_host_socket();
    let dummy_b_socket = available_ports.get_next_local_host_socket();

    // B client points at a non-existent server; it will never be called because the oversized
    // request is rejected at the HTTP layer before any component logic runs.
    let b_remote_client = ComponentBClient::new(
        RemoteClientConfig::default(),
        &dummy_b_socket.ip().to_string(),
        dummy_b_socket.port(),
        &TEST_REMOTE_CLIENT_METRICS,
    );
    let component_a = ComponentA::new(Box::new(b_remote_client));

    let (tx_a, rx_a) = channel::<RequestWrapper<ComponentARequest, ComponentAResponse>>(32);
    let a_local_client = LocalComponentClient::new(tx_a, &TEST_LOCAL_CLIENT_METRICS);

    let mut local_server = LocalComponentServer::new(
        component_a,
        &LocalServerConfig::default(),
        rx_a,
        &TEST_LOCAL_SERVER_METRICS,
    );
    task::spawn(async move {
        let _ = local_server.start().await;
    });

    let server_config = RemoteServerConfig { max_request_body_bytes: 1, ..Default::default() };
    let mut remote_server = RemoteComponentServer::new(
        a_local_client,
        server_config,
        a_socket.port(),
        &TEST_REMOTE_SERVER_METRICS,
    );
    task::spawn(async move {
        let _ = remote_server.start().await;
    });
    task::yield_now().await;

    let uri: Uri = format!("http://[{}]:{}/", a_socket.ip(), a_socket.port()).parse().unwrap();
    let http_request = Request::post(uri)
        .header(CONTENT_TYPE, APPLICATION_OCTET_STREAM)
        .header(REQUEST_ID_HEADER, RequestId::generate().to_string())
        .body(Full::new(Bytes::from("x".repeat(1024))))
        .unwrap();
    let http_response =
        Client::builder(TokioExecutor::new()).build_http().request(http_request).await.unwrap();

    assert_eq!(http_response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let body_bytes = http_response.into_body().collect().await.unwrap().to_bytes();
    let server_error = SerdeWrapper::<ServerError>::wrapper_deserialize(&body_bytes).unwrap();
    assert!(matches!(server_error, ServerError::RequestBodyTooLarge(_)));
}

/// Client returns `ResponseParsingFailure` when the server's response body exceeds
/// `max_response_body_bytes`.
#[tokio::test]
async fn response_body_too_large() {
    let socket = available_ports_factory(unique_u16!()).get_next_local_host_socket();
    task::spawn(async move {
        async fn handler(
            _http_request: Request<Incoming>,
        ) -> Result<Response<Full<Bytes>>, Infallible> {
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_TYPE, APPLICATION_OCTET_STREAM)
                .body(Full::new(Bytes::from(vec![0u8; 1024])))
                .unwrap())
        }

        let listener = TcpListener::bind(&socket).await.unwrap();
        loop {
            let Ok((stream, _)) = listener.accept().await else { continue };
            let io = TokioIo::new(stream);
            let service = service_fn(|req| async move { handler(req).await });
            tokio::spawn(async move {
                let _ = Http2ServerBuilder::new(TokioExecutor::new())
                    .http2()
                    .serve_connection(io, service)
                    .await;
            });
        }
    });
    task::yield_now().await;

    let client_config =
        RemoteClientConfig { max_response_body_bytes: 1, retries: 0, ..FAST_FAILING_CLIENT_CONFIG };
    let client = ComponentAClient::new(
        client_config,
        &socket.ip().to_string(),
        socket.port(),
        &TEST_REMOTE_CLIENT_METRICS,
    );

    let Err(error) = client.a_get_value().await else {
        panic!("Expected an error");
    };
    assert!(matches!(error, ClientError::ResponseParsingFailure(_)), "unexpected error: {error}");
}
