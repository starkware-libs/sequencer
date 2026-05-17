//! Integration tests for the TLS server bootstrap.
//!
//! Uses a self-signed certificate checked into `resources/test_tls/` (CN=localhost, valid for
//! 100 years). See `resources/test_tls/README.md` for the openssl regeneration command.

use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use jsonrpsee::server::ServerHandle;
use reqwest::StatusCode;
use rstest::rstest;
use serde_json::Value;
use tempfile::NamedTempFile;

use crate::server::config::TransportMode;
use crate::server::mock_rpc::MockProvingRpc;
use crate::server::rpc_api::ProvingRpcServer;
use crate::server::rpc_impl::SPEC_VERSION;
use crate::server::start_server;
use crate::server::tls::load_tls_acceptor;

fn ensure_crypto_provider() {
    let _ = tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();
}

fn test_cert_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/test_tls/cert.pem")
}

fn test_key_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/test_tls/key.pem")
}

fn test_mismatched_key_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/test_tls/key_mismatched.pem")
}

fn write_pem_to_tempfile(pem_bytes: &[u8]) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(pem_bytes).unwrap();
    file
}

fn spec_version_request() -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "starknet_specVersion"
    })
}

async fn start_test_tls_server() -> (SocketAddr, ServerHandle, Vec<u8>) {
    let methods = MockProvingRpc::from_expected_json().into_rpc();
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

    // Goes through `start_server` rather than `tls::start_tls_server` directly, so the
    // `TransportMode::Https` dispatch arm is covered too.
    let transport =
        TransportMode::Https { tls_cert_file: test_cert_path(), tls_key_file: test_key_path() };
    let (local_addr, handle) = start_server(
        addr,
        &transport,
        methods.into(),
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

    // Connect via the bound address, not `localhost`: the name may resolve to `::1` while the
    // server listens on `127.0.0.1`. The fixture cert's SAN covers `IP:127.0.0.1`.
    let response = client
        .post(format!("https://{addr}"))
        .json(&spec_version_request())
        .send()
        .await
        .expect("HTTPS request failed");

    assert_eq!(response.status(), StatusCode::OK);
    let json: Value = response.json().await.unwrap();
    assert_eq!(json["result"].as_str().unwrap(), SPEC_VERSION);

    handle.stop().unwrap();
}

#[tokio::test]
async fn test_http_to_tls_server_fails() {
    ensure_crypto_provider();
    let (addr, handle, _cert_pem) = start_test_tls_server().await;

    // Plain HTTP to a TLS server should fail (connection or protocol error). Connect via the
    // bound address, not `localhost`, which may resolve to `::1`.
    let result = reqwest::Client::new()
        .post(format!("http://{addr}"))
        .json(&spec_version_request())
        .send()
        .await;
    assert!(result.is_err(), "Expected HTTP to TLS server to fail, got: {result:?}");

    handle.stop().unwrap();
}

/// How a given path argument is produced for `load_tls_acceptor`.
enum PathMode {
    /// Use the checked-in valid test fixture.
    Valid,
    /// Path to a file that does not exist.
    Missing,
    /// Path to the checked-in key that is valid PEM but does not match `cert.pem`.
    MismatchedKey,
    /// Path to a tempfile containing these bytes.
    Junk(&'static [u8]),
}

/// Returns the `NamedTempFile` alongside the path so the caller keeps it alive for the call.
fn resolve_pem_path(
    mode: PathMode,
    missing_path: &str,
    valid_path: PathBuf,
) -> (PathBuf, Option<NamedTempFile>) {
    match mode {
        PathMode::Valid => (valid_path, None),
        PathMode::Missing => (missing_path.into(), None),
        PathMode::MismatchedKey => (test_mismatched_key_path(), None),
        PathMode::Junk(bytes) => {
            let file = write_pem_to_tempfile(bytes);
            (file.path().into(), Some(file))
        }
    }
}

/// Expected substrings are taken verbatim from the `.context(...)`/`bail!` wording in
/// `load_tls_acceptor`.
#[rstest]
#[case::missing_cert(PathMode::Missing, PathMode::Valid, "Failed to read TLS certificate file")]
#[case::missing_key(PathMode::Valid, PathMode::Missing, "Failed to read TLS private key file")]
#[case::invalid_cert_pem(
    PathMode::Junk(b"not a valid PEM cert"),
    PathMode::Valid,
    "does not contain any certificate PEM blocks"
)]
#[case::invalid_key_pem(
    PathMode::Valid,
    PathMode::Junk(b"not a valid PEM key"),
    "Failed to parse TLS private key PEM from"
)]
#[case::mismatched_key(
    PathMode::Valid,
    PathMode::MismatchedKey,
    "Failed to construct TLS server configuration from certificate and key"
)]
fn test_load_tls_acceptor_failure(
    #[case] cert: PathMode,
    #[case] key: PathMode,
    #[case] expected_error_substring: &str,
) {
    // `PathMode::MismatchedKey` reaches rustls' `ServerConfig` builder, which requires a
    // process-level crypto provider. nextest runs each test in a fresh process, so install the
    // provider here even though the other cases error out before reaching it.
    ensure_crypto_provider();

    let (cert_path, _cert_tempfile) =
        resolve_pem_path(cert, "/nonexistent/cert.pem", test_cert_path());
    let (key_path, _key_tempfile) = resolve_pem_path(key, "/nonexistent/key.pem", test_key_path());

    let Err(error) = load_tls_acceptor(&cert_path, &key_path) else {
        panic!("expected an error");
    };
    assert!(
        error.to_string().contains(expected_error_substring),
        "expected error to contain {expected_error_substring:?}, got: {error}"
    );
}

#[test]
fn test_load_tls_acceptor_succeeds_for_valid_files() {
    // `load_tls_acceptor` builds a rustls `ServerConfig`, which requires a process-level crypto
    // provider. nextest runs each test in a fresh process, so install the provider here.
    ensure_crypto_provider();
    load_tls_acceptor(&test_cert_path(), &test_key_path()).unwrap();
}
