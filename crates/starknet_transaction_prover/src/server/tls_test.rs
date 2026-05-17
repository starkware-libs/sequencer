//! Integration tests for the TLS server bootstrap.
//!
//! Uses a self-signed certificate checked into `resources/test_tls/`. The cert covers
//! `CN=localhost` (+ SAN `DNS:localhost`) and is valid for 100 years; regenerate with:
//!
//! ```bash
//! openssl req -x509 -newkey rsa:2048 \
//!   -keyout crates/starknet_transaction_prover/resources/test_tls/key.pem \
//!   -out   crates/starknet_transaction_prover/resources/test_tls/cert.pem \
//!   -sha256 -days 36500 -nodes \
//!   -subj "/CN=localhost" \
//!   -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"
//! ```

use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use serde_json::Value;
use tempfile::NamedTempFile;

use crate::server::mock_rpc::MockProvingRpc;
use crate::server::rpc_api::ProvingRpcServer;
use crate::server::rpc_impl::SPEC_VERSION;
use crate::server::tls::{load_tls_acceptor, start_tls_server};

/// Installs the default rustls crypto provider (aws-lc-rs) if not already installed.
/// Required by reqwest when using rustls-based TLS.
fn ensure_crypto_provider() {
    let _ = tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();
}

fn test_cert_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/test_tls/cert.pem")
}

fn test_key_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/test_tls/key.pem")
}

/// Reads the checked-in self-signed test certificate as PEM bytes.
fn read_test_cert_pem() -> Vec<u8> {
    std::fs::read(test_cert_path()).expect("Failed to read test cert.pem")
}

/// Writes PEM bytes to a temporary file and returns the handle.
fn write_pem_to_tempfile(pem_bytes: &[u8]) -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("Failed to create temp file");
    file.write_all(pem_bytes).expect("Failed to write PEM");
    file.flush().expect("Failed to flush PEM file");
    file
}

/// Starts a TLS server with mock RPC methods, returns (addr, server_handle, cert_pem).
async fn start_test_tls_server() -> (SocketAddr, jsonrpsee::server::ServerHandle, Vec<u8>) {
    let methods = MockProvingRpc::from_expected_json().into_rpc();
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

    let (local_addr, handle) = start_tls_server(
        addr,
        &test_cert_path(),
        &test_key_path(),
        methods,
        10,              // max_connections
        5 * 1024 * 1024, // max_request_body_size
        None,            // cors_layer
        None,            // ohttp_layer
    )
    .await
    .expect("Failed to start TLS server");

    (local_addr, handle, read_test_cert_pem())
}

#[tokio::test]
async fn test_https_spec_version_succeeds() {
    ensure_crypto_provider();
    let (addr, handle, cert_pem) = start_test_tls_server().await;

    let cert = reqwest::tls::Certificate::from_pem(&cert_pem)
        .expect("Failed to parse certificate for reqwest");
    let client = reqwest::Client::builder()
        .add_root_certificate(cert)
        .build()
        .expect("Failed to build HTTPS client");

    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "starknet_specVersion"
    });

    let response = client
        .post(format!("https://localhost:{}", addr.port()))
        .json(&body)
        .send()
        .await
        .expect("HTTPS request failed");

    assert_eq!(response.status(), 200);

    let json: Value = response.json().await.expect("Failed to parse response JSON");
    assert_eq!(json["result"].as_str().unwrap(), SPEC_VERSION);

    handle.stop().expect("Failed to stop server");
}

#[tokio::test]
async fn test_http_to_tls_server_fails() {
    ensure_crypto_provider();
    let (addr, handle, _cert_pem) = start_test_tls_server().await;

    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "starknet_specVersion"
    });

    // Plain HTTP to a TLS server should fail (connection or protocol error).
    let result = client.post(format!("http://localhost:{}", addr.port())).json(&body).send().await;

    assert!(result.is_err(), "Expected HTTP to TLS server to fail, but got: {result:?}");

    handle.stop().expect("Failed to stop server");
}

#[test]
fn test_load_tls_acceptor_missing_cert_file() {
    let key_file = write_pem_to_tempfile(b"dummy key content");
    let result = load_tls_acceptor("/nonexistent/cert.pem".as_ref(), key_file.path());
    assert!(result.is_err(), "Expected error for missing cert file");
}

#[test]
fn test_load_tls_acceptor_missing_key_file() {
    let cert_file = write_pem_to_tempfile(b"dummy cert content");
    let result = load_tls_acceptor(cert_file.path(), "/nonexistent/key.pem".as_ref());
    assert!(result.is_err(), "Expected error for missing key file");
}

#[test]
fn test_load_tls_acceptor_invalid_pem() {
    let cert_file = write_pem_to_tempfile(b"not a valid PEM certificate");
    let key_file = write_pem_to_tempfile(b"not a valid PEM key");
    let result = load_tls_acceptor(cert_file.path(), key_file.path());
    assert!(result.is_err(), "Expected error for invalid PEM content");
}

#[test]
fn test_load_tls_acceptor_succeeds_for_valid_files() {
    // Sanity check that the checked-in test cert/key actually load as a TLS acceptor.
    load_tls_acceptor(&test_cert_path(), &test_key_path())
        .expect("Expected test cert/key to load successfully");
}
