//! End-to-end check that a termination signal drains the running server.
//!
//! This lives in its own integration-test binary on purpose. The test sends a
//! real `SIGTERM` to its own process, and a signal is process-wide. Sharing a
//! process with the unit tests would let the signal reach another test's
//! server, and would race the handler registration that makes sending it safe
//! at all.

use std::net::SocketAddr;
use std::process::Command;
use std::time::Duration;

use jsonrpsee::server::{ServerBuilder, ServerHandle};
use jsonrpsee::RpcModule;
use starknet_transaction_prover::server::shutdown::spawn_signal_bridge;

async fn start_bare_server() -> ServerHandle {
    let addr: SocketAddr = "127.0.0.1:0".parse().expect("valid loopback address");
    let server =
        ServerBuilder::default().build(addr).await.expect("failed to bind test JSON-RPC server");
    let methods: jsonrpsee::Methods = RpcModule::new(()).into();
    server.start(methods)
}

/// The first `SIGTERM` must call `stop()` on the server. The unit tests cover
/// only the inner race helper, so this one covers the path from a real signal
/// through to a stopped server.
#[tokio::test]
async fn sigterm_stops_the_running_server() {
    let server_handle = start_bare_server().await;
    // Registers the tokio signal handlers, which also replaces the default
    // terminate-the-process disposition, so the signal below is safe to send.
    spawn_signal_bridge(server_handle.clone()).expect("signal handlers must install");

    let status = Command::new("kill")
        .args(["-TERM", &std::process::id().to_string()])
        .status()
        .expect("failed to run kill");
    assert!(status.success(), "kill -TERM failed: {status}");

    tokio::time::timeout(Duration::from_secs(10), server_handle.stopped())
        .await
        .expect("SIGTERM must stop the server");
}
