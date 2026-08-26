//! Tests for `start_server`'s wiring of the optional middleware layers.

use std::path::{Path, PathBuf};

use reqwest::header;
use serde_json::Value;

use crate::server::config::{TransportMode, DEFAULT_MAX_REQUEST_BODY_SIZE};
use crate::server::cors::build_cors_layer;
use crate::server::start_server;
use crate::server::test_utils::{
    ensure_crypto_provider,
    jsonrpc_request,
    loopback_addr,
    mock_rpc_module,
    TEST_MAX_CONNECTIONS,
};

const ALLOWED_ORIGIN: &str = "http://example.com";

fn test_tls_path(file_name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/test_tls").join(file_name)
}

#[tokio::test]
async fn start_server_attaches_the_cors_layer() {
    let cors_layer = build_cors_layer(&[ALLOWED_ORIGIN.to_string()])
        .unwrap()
        .expect("a non-empty allowlist enables CORS");
    let (addr, handle) = start_server(
        loopback_addr(),
        &TransportMode::Http,
        mock_rpc_module().into(),
        TEST_MAX_CONNECTIONS,
        DEFAULT_MAX_REQUEST_BODY_SIZE,
        Some(cors_layer),
        None, // ohttp_layer
    )
    .await
    .expect("failed to start HTTP server");

    let response = reqwest::Client::new()
        .post(format!("http://{addr}"))
        .header(header::ORIGIN, ALLOWED_ORIGIN)
        .json(&jsonrpc_request("starknet_specVersion", Value::Null))
        .send()
        .await
        .expect("HTTP request failed");

    assert_eq!(
        response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).map(|v| v.to_str().unwrap()),
        Some(ALLOWED_ORIGIN)
    );

    handle.stop().unwrap();
}

/// The `Https` arm forwards to `tls::start_tls_server` separately, so the plaintext case above
/// does not cover it — and HTTPS is what gets deployed.
#[tokio::test]
async fn start_server_attaches_the_cors_layer_over_tls() {
    ensure_crypto_provider();
    let cors_layer = build_cors_layer(&[ALLOWED_ORIGIN.to_string()])
        .unwrap()
        .expect("a non-empty allowlist enables CORS");
    let transport = TransportMode::Https {
        tls_cert_file: test_tls_path("cert.pem"),
        tls_key_file: test_tls_path("key.pem"),
    };
    let (addr, handle) = start_server(
        loopback_addr(),
        &transport,
        mock_rpc_module().into(),
        TEST_MAX_CONNECTIONS,
        DEFAULT_MAX_REQUEST_BODY_SIZE,
        Some(cors_layer),
        None, // ohttp_layer
    )
    .await
    .expect("failed to start HTTPS server");

    let cert =
        reqwest::tls::Certificate::from_pem(&std::fs::read(test_tls_path("cert.pem")).unwrap())
            .unwrap();
    let response = reqwest::Client::builder()
        .add_root_certificate(cert)
        .build()
        .unwrap()
        .post(format!("https://{addr}"))
        .header(header::ORIGIN, ALLOWED_ORIGIN)
        .json(&jsonrpc_request("starknet_specVersion", Value::Null))
        .send()
        .await
        .expect("HTTPS request failed");

    assert_eq!(
        response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).map(|v| v.to_str().unwrap()),
        Some(ALLOWED_ORIGIN)
    );

    handle.stop().unwrap();
}

/// Guards the two cases above against passing on a response that carries the header
/// unconditionally.
#[tokio::test]
async fn start_server_omits_cors_headers_when_no_layer_is_configured() {
    let (addr, handle) = start_server(
        loopback_addr(),
        &TransportMode::Http,
        mock_rpc_module().into(),
        TEST_MAX_CONNECTIONS,
        DEFAULT_MAX_REQUEST_BODY_SIZE,
        None, // cors_layer
        None, // ohttp_layer
    )
    .await
    .expect("failed to start HTTP server");

    let response = reqwest::Client::new()
        .post(format!("http://{addr}"))
        .header(header::ORIGIN, ALLOWED_ORIGIN)
        .json(&jsonrpc_request("starknet_specVersion", Value::Null))
        .send()
        .await
        .expect("HTTP request failed");

    assert!(response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());

    handle.stop().unwrap();
}
