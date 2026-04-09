use std::time::Duration;

use apollo_proc_macros::unique_u16;
use tokio::io::AsyncReadExt;
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
