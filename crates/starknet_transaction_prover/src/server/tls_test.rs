//! Integration tests for the TLS server bootstrap.
//!
//! Uses a self-signed certificate checked into `resources/test_tls/` (CN=localhost, valid for
//! 100 years). See `resources/test_tls/README.md` for the openssl regeneration command.

use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use rstest::rstest;
use serde_json::Value;
use tempfile::NamedTempFile;

use crate::server::mock_rpc::MockProvingRpc;
use crate::server::rpc_api::ProvingRpcServer;
use crate::server::rpc_impl::SPEC_VERSION;
use crate::server::tls::{load_tls_acceptor, start_tls_server};

fn ensure_crypto_provider() {
    let _ = tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();
}

fn test_cert_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/test_tls/cert.pem")
}

fn test_key_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/test_tls/key.pem")
}

fn write_pem_to_tempfile(pem_bytes: &[u8]) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(pem_bytes).unwrap();
    file.flush().unwrap();
    file
}

fn spec_version_request() -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "starknet_specVersion"
    })
}

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

    let cert_pem = std::fs::read(test_cert_path()).unwrap();
    (local_addr, handle, cert_pem)
}

#[tokio::test]
async fn test_https_spec_version_succeeds() {
    ensure_crypto_provider();
    let (addr, handle, cert_pem) = start_test_tls_server().await;

    let cert = reqwest::tls::Certificate::from_pem(&cert_pem).unwrap();
    let client = reqwest::Client::builder().add_root_certificate(cert).build().unwrap();

    let response = client
        .post(format!("https://localhost:{}", addr.port()))
        .json(&spec_version_request())
        .send()
        .await
        .expect("HTTPS request failed");

    assert_eq!(response.status(), 200);
    let json: Value = response.json().await.unwrap();
    assert_eq!(json["result"].as_str().unwrap(), SPEC_VERSION);

    handle.stop().unwrap();
}

#[tokio::test]
async fn test_http_to_tls_server_fails() {
    ensure_crypto_provider();
    let (addr, handle, _cert_pem) = start_test_tls_server().await;

    // Plain HTTP to a TLS server should fail (connection or protocol error).
    let result = reqwest::Client::new()
        .post(format!("http://localhost:{}", addr.port()))
        .json(&spec_version_request())
        .send()
        .await;
    assert!(result.is_err(), "Expected HTTP to TLS server to fail, got: {result:?}");

    handle.stop().unwrap();
}

/// How a given path argument is materialised for `load_tls_acceptor`.
enum PathMode {
    /// Use the checked-in valid test fixture.
    Valid,
    /// Path to a file that does not exist.
    Missing,
    /// Path to a tempfile containing these bytes (returned alongside the path so the
    /// `NamedTempFile` is kept alive for the call).
    Junk(&'static [u8]),
}

/// `PathMode::Junk` returns `Some(tempfile)` so the tempfile is dropped after the test, not before.
fn materialise(mode: PathMode, missing: &str, valid: PathBuf) -> (PathBuf, Option<NamedTempFile>) {
    match mode {
        PathMode::Valid => (valid, None),
        PathMode::Missing => (missing.into(), None),
        PathMode::Junk(bytes) => {
            let file = write_pem_to_tempfile(bytes);
            (file.path().into(), Some(file))
        }
    }
}

/// Each case isolates one specific failure path by holding the other input valid, so a green test
/// proves `load_tls_acceptor` actually rejected on the named reason and not on something earlier.
#[rstest]
#[case::missing_cert(PathMode::Missing, PathMode::Valid)]
#[case::missing_key(PathMode::Valid, PathMode::Missing)]
#[case::invalid_cert_pem(PathMode::Junk(b"not a valid PEM cert"), PathMode::Valid)]
#[case::invalid_key_pem(PathMode::Valid, PathMode::Junk(b"not a valid PEM key"))]
fn test_load_tls_acceptor_failure(#[case] cert: PathMode, #[case] key: PathMode) {
    let (cert_path, _cert_tmp) = materialise(cert, "/nonexistent/cert.pem", test_cert_path());
    let (key_path, _key_tmp) = materialise(key, "/nonexistent/key.pem", test_key_path());

    assert!(load_tls_acceptor(&cert_path, &key_path).is_err());
}

#[test]
fn test_load_tls_acceptor_succeeds_for_valid_files() {
    // `load_tls_acceptor` builds a rustls `ServerConfig`, which requires a process-level crypto
    // provider. nextest runs each test in a fresh process, so install the provider here.
    ensure_crypto_provider();
    load_tls_acceptor(&test_cert_path(), &test_key_path()).unwrap();
}
