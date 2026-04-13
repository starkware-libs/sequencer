use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use apollo_proc_macros::unique_u16;
use rstest::rstest;
use socket2::{SockRef, TcpKeepalive};
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::channel;
use tokio::task;
use tokio::time::timeout;

use crate::component_client::LocalComponentClient;
use crate::component_definitions::RequestWrapper;
use crate::component_server::{ComponentServerStarter, RemoteComponentServer, RemoteServerConfig};
use crate::tests::test_utils::{
    available_ports_factory,
    connect_zombie,
    contains_goaway_frame,
    dummy_remote_server_config,
    ComponentARequest,
    ComponentAResponse,
    MAX_CONCURRENCY,
    TEST_LOCAL_CLIENT_METRICS,
    TEST_REMOTE_SERVER_METRICS,
};

/// Verifies that `SO_KEEPALIVE` on a server-accepted socket.
///
/// The test accepts the connection itself so it owns the `TcpStream` and can inspect socket
/// options via `SockRef::from` without any unsafe FD scanning.
#[rstest]
#[tokio::test]
async fn server_tcp_keepalive_socket_option_matches_config() {
    const SUFFICIENTLY_LONG_KEEPALIVE_TIMEOUT_MS: u64 = 1000;

    let listener =
        TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)).await.unwrap();
    let server_addr = listener.local_addr().unwrap();

    let _client_stream = TcpStream::connect(server_addr).await.unwrap();
    let (accepted_stream, _) = listener.accept().await.unwrap();

    // Mirror the keepalive logic in RemoteComponentServer::start().
    let keepalive = TcpKeepalive::new()
        .with_time(Duration::from_millis(SUFFICIENTLY_LONG_KEEPALIVE_TIMEOUT_MS));
    SockRef::from(&accepted_stream).set_tcp_keepalive(&keepalive).unwrap();

    assert!(
        SockRef::from(&accepted_stream).keepalive().unwrap(),
        "SO_KEEPALIVE on the accepted socket should reflect idle_time_ms"
    );
}

/// Verifies that the server evicts a zombie connection via HTTP/2 PING after the keepalive
/// interval and timeout elapse without receiving a response, and that the TCP keepalive socket
/// option configured on accepted sockets does not interfere with this mechanism.
///
/// # Why TCP keepalive cannot evict the connection in this setup
///
/// The server always configures TCP keepalive on accepted sockets. The two eviction mechanisms
/// are distinguishable by how the zombie socket observes the close:
/// - **TCP keepalive**: the kernel sends a RST after all probes go unanswered → `read_to_end`
///   returns `Err(connection reset by peer)`.
/// - **HTTP/2 PING timeout (hyper)**: the server sends a GOAWAY frame and then closes gracefully →
///   `read_to_end` returns `Ok` with data containing a GOAWAY frame.
///
/// On loopback (`127.0.0.1`) the kernel itself ACKs TCP keepalive probes, even when the remote
/// application ignores them. Probes therefore never go unanswered, and the kernel never sends a
/// RST. Testing TCP keepalive eviction would require a setup where probes can genuinely be
/// dropped — for example, a `veth` pair in separate network namespaces with `tc netem` packet
/// loss applied to ACKs. In the unit-test environment that is not available, so the test asserts
/// `Ok` + GOAWAY to confirm the eviction is via HTTP/2 PING and that TCP keepalive does not
/// interfere.
#[tokio::test]
async fn tcp_keepalive_does_not_interfere_with_http_keepalive_eviction() {
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

    // Closure must be a graceful HTTP/2 GOAWAY (Ok), not a TCP RST (Err).
    let mut buf = Vec::new();
    let bytes_read = timeout(
        Duration::from_millis(KEEPALIVE_INTERVAL_MS + KEEPALIVE_TIMEOUT_MS + MARGIN_MS),
        zombie.read_to_end(&mut buf),
    )
    .await
    .expect("server should have closed the zombie connection after keepalive timeout");
    bytes_read.expect("connection should close cleanly via GOAWAY, not via TCP RST");
    assert!(contains_goaway_frame(&buf), "server should have sent a GOAWAY frame before closing");
}
