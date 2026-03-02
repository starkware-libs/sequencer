use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use tempfile::NamedTempFile;
use tokio_rustls::rustls::crypto::aws_lc_rs;

use crate::config::ProverConfig;
use crate::server::config::{ServiceConfig, TransportMode};
use crate::server::rpc_impl::ProvingRpcServerImpl;
use crate::server::rpc_trait::ProvingRpcServer;
use crate::server::tls::start_tls_server;

/// Installs the aws-lc-rs crypto provider for rustls (required by rcgen and reqwest). Safe to call
/// from multiple tests — the second call is a no-op.
fn ensure_crypto_provider() {
    let _ = aws_lc_rs::default_provider().install_default();
}

/// Generates a self-signed certificate and private key for localhost, writes them to temporary
/// files, and returns the handles (kept alive so the files persist for the test duration).
fn generate_self_signed_cert_files() -> (NamedTempFile, NamedTempFile) {
    let certified_key =
        rcgen::generate_simple_self_signed(vec!["localhost".to_string(), "127.0.0.1".to_string()])
            .expect("failed to generate self-signed cert");

    let mut cert_file = NamedTempFile::new().expect("failed to create temp cert file");
    cert_file.write_all(certified_key.cert.pem().as_bytes()).expect("failed to write cert PEM");
    cert_file.flush().expect("failed to flush cert file");

    let mut key_file = NamedTempFile::new().expect("failed to create temp key file");
    key_file
        .write_all(certified_key.key_pair.serialize_pem().as_bytes())
        .expect("failed to write key PEM");
    key_file.flush().expect("failed to flush key file");

    (cert_file, key_file)
}

/// Returns a JSON-RPC 2.0 request body for `starknet_specVersion`.
fn spec_version_request_body() -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "starknet_specVersion",
        "params": []
    })
}

/// Builds RPC methods using a dummy config. Only `spec_version` is exercised, so the dummy prover
/// URL is never contacted.
fn build_test_rpc_methods() -> jsonrpsee::server::Methods {
    let config = ServiceConfig {
        prover_config: ProverConfig {
            rpc_node_url: "http://localhost:1".to_string(),
            ..ProverConfig::default()
        },
        ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: 0,
        max_concurrent_requests: 1,
        max_connections: 10,
        cors_allow_origin: Vec::new(),
        transport: TransportMode::Http,
    };
    ProvingRpcServerImpl::from_config(&config).into_rpc().into()
}

#[tokio::test]
async fn tls_server_serves_https_spec_version() {
    ensure_crypto_provider();
    let (cert_file, key_file) = generate_self_signed_cert_files();
    let methods = build_test_rpc_methods();

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let (local_addr, server_handle) =
        start_tls_server(addr, cert_file.path(), key_file.path(), methods, 10, None)
            .await
            .expect("failed to start TLS server");

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("failed to build HTTPS client");

    let response = client
        .post(format!("https://127.0.0.1:{}", local_addr.port()))
        .json(&spec_version_request_body())
        .send()
        .await
        .expect("HTTPS request failed");

    assert!(response.status().is_success(), "expected 2xx, got {}", response.status());

    let body: serde_json::Value = response.json().await.expect("failed to parse response JSON");
    assert_eq!(body["result"], "0.10.0", "unexpected spec version: {body}");

    server_handle.stop().expect("failed to stop server");
}

#[tokio::test]
async fn tls_server_rejects_plain_http() {
    ensure_crypto_provider();
    let (cert_file, key_file) = generate_self_signed_cert_files();
    let methods = build_test_rpc_methods();

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let (local_addr, server_handle) =
        start_tls_server(addr, cert_file.path(), key_file.path(), methods, 10, None)
            .await
            .expect("failed to start TLS server");

    let client = reqwest::Client::new();

    let result = client
        .post(format!("http://127.0.0.1:{}", local_addr.port()))
        .json(&spec_version_request_body())
        .send()
        .await;

    assert!(result.is_err(), "expected plain HTTP to fail against TLS server, but got: {result:?}");

    server_handle.stop().expect("failed to stop server");
}
