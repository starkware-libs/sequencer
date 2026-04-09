use std::convert::Infallible;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use apollo_proc_macros::unique_u16;
use bytes::Bytes;
use http::header::CONTENT_TYPE;
use http::StatusCode;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as Http2ServerBuilder;
use tokio::net::TcpListener;
use tokio::task;
use tokio::time::sleep;

use crate::component_client::RemoteClientConfig;
use crate::component_definitions::APPLICATION_OCTET_STREAM;
use crate::serde_utils::SerdeWrapper;
use crate::tests::test_utils::{
    available_ports_factory,
    ComponentAClient,
    ComponentAClientTrait,
    ComponentAResponse,
    TEST_REMOTE_CLIENT_METRICS,
    VALID_VALUE_A,
};

/// Verifies that Hyper evicts an idle connection from its pool after the keepalive timeout
/// and opens a fresh TCP connection for the next request.
///
/// Since no pool timer is configured, eviction is triggered by the next pool checkout (the
/// second request), not a background task. A new connection is detected by counting
/// server-side accepts.
#[tokio::test]
async fn idle_connection_is_evicted_after_pool_timeout() {
    const KEEPALIVE_TIMEOUT_MS: u64 = 100;
    const MARGIN_MS: u64 = 300;

    let socket = available_ports_factory(unique_u16!()).get_next_local_host_socket();
    let accept_count = Arc::new(AtomicUsize::new(0));

    // A server that accepts connections and keeps track of the number of connections it has
    // accepted.
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
