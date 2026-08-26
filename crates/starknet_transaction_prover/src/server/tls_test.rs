//! Drives [`super::spawn_accept_loop`] over plain TCP, with an identity `prepare_stream` in place
//! of the TLS handshake so no certificate is needed.

use std::sync::Arc;
use std::time::Duration;

use jsonrpsee::core::RpcResult;
use jsonrpsee::server::{stop_channel, Methods, ServerConfig};
use jsonrpsee::RpcModule;
use tokio::net::TcpListener;
use tokio::sync::Notify;

use super::spawn_accept_loop;

/// Lets the test hold a request inside the handler for as long as it needs.
struct ParkedHandler {
    entered: Notify,
    release: Notify,
}

/// Handing `build` a different `StopHandle` in `spawn_accept_loop` would let `stopped()` resolve
/// mid-request, dropping the runtime on top of an in-flight proof, and would fail this test.
#[tokio::test]
async fn stopped_waits_for_an_in_flight_request_to_finish() {
    let handler = Arc::new(ParkedHandler { entered: Notify::new(), release: Notify::new() });

    let mut module = RpcModule::new(handler.clone());
    module
        .register_async_method("test_park", |_params, handler, _extensions| async move {
            handler.entered.notify_one();
            handler.release.notified().await;
            RpcResult::Ok("released".to_string())
        })
        .expect("Failed to register the parking method");

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("Failed to bind test listener");
    let local_addr = listener.local_addr().expect("Failed to read the listener address");
    let (stop_handle, server_handle) = stop_channel();

    spawn_accept_loop(
        listener,
        stop_handle,
        Methods::from(module),
        ServerConfig::builder().build(),
        None,
        None,
        |socket, _remote_addr| async move { Some(socket) },
    );

    let request = tokio::spawn(async move {
        reqwest::Client::new()
            .post(format!("http://{local_addr}"))
            .header("content-type", "application/json")
            .body(r#"{"jsonrpc":"2.0","id":1,"method":"test_park","params":[]}"#)
            .send()
            .await
            .expect("Request failed")
            .text()
            .await
            .expect("Failed to read the response body")
    });

    handler.entered.notified().await;
    server_handle.stop().expect("Failed to stop the server");

    // The accept loop has dropped its handle by now, so only the draining connection holds one.
    let resolved_early =
        tokio::time::timeout(Duration::from_millis(300), server_handle.clone().stopped()).await;
    assert!(resolved_early.is_err(), "stopped() resolved while a request was still in flight");

    handler.release.notify_one();
    let body = request.await.expect("Request task panicked");
    assert!(
        body.contains("released"),
        "the in-flight request did not complete across the stop: {body}"
    );

    tokio::time::timeout(Duration::from_secs(5), server_handle.stopped())
        .await
        .expect("stopped() never resolved after the in-flight request finished");
}
