use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use apollo_proc_macros::unique_u16;
use bytes::Bytes;
use http::StatusCode;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as Http2ServerBuilder;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::channel;
use tokio::sync::Mutex;
use tokio::task;
use tokio::time::{sleep, timeout};

use crate::component_client::{
    ClientError,
    LocalComponentClient,
    RemoteClientConfig,
    RemoteComponentClient,
};
use crate::component_definitions::{ComponentClient, RequestWrapper};
use crate::component_server::{ComponentServerStarter, RemoteComponentServer, RemoteServerConfig};
use crate::serde_utils::SerdeWrapper;
use crate::tests::component_a_b_fixture::{ComponentARequest, ComponentAResponse, VALID_VALUE_A};
use crate::tests::{
    available_ports_factory,
    dummy_remote_server_config,
    TEST_LOCAL_CLIENT_METRICS,
    TEST_REMOTE_CLIENT_METRICS,
    TEST_REMOTE_SERVER_METRICS,
};

type ComponentAClient = RemoteComponentClient<ComponentARequest, ComponentAResponse>;

#[tokio::test]
async fn retry_request() {
    let socket = available_ports_factory(unique_u16!()).get_next_local_host_socket();
    // Spawn a server that responds with OK every other request.
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
    let ComponentAResponse::AGetValue(value) =
        a_client_retry.send(ComponentARequest::AGetValue).await.unwrap();
    assert_eq!(value, VALID_VALUE_A);

    // The current server state is 'false', hence the first and only attempt returns an error.
    let no_retry_config = RemoteClientConfig { retries: 0, ..Default::default() };
    let a_client_no_retry = ComponentAClient::new(
        no_retry_config,
        &socket.ip().to_string(),
        socket.port(),
        &TEST_REMOTE_CLIENT_METRICS,
    );
    let Err(error): Result<ComponentAResponse, ClientError> =
        a_client_no_retry.send(ComponentARequest::AGetValue).await
    else {
        panic!("Expected an error.");
    };
    assert!(
        error.to_string().contains(StatusCode::IM_A_TEAPOT.as_str()),
        "Expected IM_A_TEAPOT in error, got: {error}"
    );
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

    // Start a RemoteComponentServer with very short keepalive values.
    // The local channel receiver is intentionally dropped — no requests will be sent.
    let (tx, _rx) = channel::<RequestWrapper<ComponentARequest, ComponentAResponse>>(32);
    let local_client = LocalComponentClient::<ComponentARequest, ComponentAResponse>::new(
        tx,
        &TEST_LOCAL_CLIENT_METRICS,
    );
    let config = RemoteServerConfig {
        keepalive_interval_ms: KEEPALIVE_INTERVAL_MS,
        keepalive_timeout_ms: KEEPALIVE_TIMEOUT_MS,
        ..dummy_remote_server_config(socket.ip())
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

    // Wait for the keepalive cycle to fire and time out.
    sleep(Duration::from_millis(KEEPALIVE_INTERVAL_MS + KEEPALIVE_TIMEOUT_MS + MARGIN_MS)).await;

    // The server should have closed the connection; read_to_end should return quickly with
    // whatever GOAWAY bytes were sent, and then EOF.
    let mut remainder = Vec::new();
    let read_result =
        timeout(Duration::from_millis(MARGIN_MS), zombie.read_to_end(&mut remainder)).await;
    assert!(
        read_result.is_ok(),
        "Server should have closed the zombie connection after keepalive timeout, but the \
         connection is still open"
    );
}
