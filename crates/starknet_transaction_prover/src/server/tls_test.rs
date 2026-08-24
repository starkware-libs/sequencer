//! Covers the drain behavior of the accept loop in [`super::start_tls_server`].
//!
//! The loop runs over plain TCP here. A TLS handshake needs a certificate on disk and takes no
//! part in the drain: what keeps the process alive is the per-connection `StopHandle` clone that
//! the request service holds until the connection finishes.

use std::sync::Arc;
use std::time::Duration;

use jsonrpsee::core::RpcResult;
use jsonrpsee::server::{
    serve_with_graceful_shutdown,
    stop_channel,
    Methods,
    ServerBuilder,
    ServerConfig,
};
use jsonrpsee::RpcModule;
use tokio::net::TcpListener;
use tokio::sync::Notify;

/// Lets the test hold a request inside the handler for as long as it needs.
struct ParkedHandler {
    entered: Notify,
    release: Notify,
}

/// `ServerHandle::stopped()` resolves once every `StopHandle` clone is dropped, and the service
/// serving a connection holds one until that connection is done. So a request that is still
/// running keeps `stopped()` pending, which is what stops `main` from returning and dropping the
/// runtime out from under an in-flight proof. Removing the `stop_handle.clone()` handed to
/// `build` would abort in-flight HTTPS requests on shutdown, and this test would fail.
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

    let svc_builder =
        ServerBuilder::default().set_config(ServerConfig::builder().build()).to_service_builder();
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("Failed to bind test listener");
    let local_addr = listener.local_addr().expect("Failed to read the listener address");
    let methods: Methods = module.into();
    let (stop_handle, server_handle) = stop_channel();

    // Same shape as `start_tls_server`'s accept loop, without the handshake.
    tokio::spawn(async move {
        loop {
            let accept_result = tokio::select! {
                accept_result = listener.accept() => accept_result,
                _ = stop_handle.clone().shutdown() => break,
            };
            let Ok((socket, _)) = accept_result else { continue };

            let stop_handle = stop_handle.clone();
            let methods = methods.clone();
            let svc_builder = svc_builder.clone();
            tokio::spawn(async move {
                let svc = svc_builder.build(methods, stop_handle.clone());
                let _ = serve_with_graceful_shutdown(socket, svc, stop_handle.shutdown()).await;
            });
        }
    });

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

    // The accept loop breaks as soon as it sees the stop, so anything still holding the channel
    // open past this point is the draining connection.
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
