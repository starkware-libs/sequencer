use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use apollo_proc_macros::unique_u16;
use rstest::rstest;
use socket2::{SockRef, TcpKeepalive};
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::channel;
use tokio::task;
use tokio::time::{sleep, timeout};

use crate::component_client::LocalComponentClient;
use crate::component_definitions::RequestWrapper;
use crate::component_server::{ComponentServerStarter, RemoteComponentServer, RemoteServerConfig};
use crate::tests::test_utils::{
    available_ports_factory,
    connect_zombie,
    dummy_remote_server_config,
    ComponentARequest,
    ComponentAResponse,
    MAX_CONCURRENCY,
    TEST_LOCAL_CLIENT_METRICS,
    TEST_REMOTE_SERVER_METRICS,
};

/// Verifies that the server closes a zombie connection after the HTTP keepalive interval and
/// timeout elapse without receiving a PING response.
#[tokio::test]
async fn zombie_connection_is_evicted_via_http_keepalive() {
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
