//! Shared helpers for the server test modules.

use std::net::SocketAddr;

use jsonrpsee::RpcModule;

use crate::server::mock_rpc::MockProvingRpc;
use crate::server::rpc_api::ProvingRpcServer;

/// Connection cap for test servers; ample for the handful of requests a test makes.
pub const TEST_MAX_CONNECTIONS: u32 = 10;

/// A jsonrpsee module backed by the mock prover RPC with canned expected responses.
pub fn mock_rpc_module() -> RpcModule<MockProvingRpc> {
    MockProvingRpc::from_expected_json().into_rpc()
}

/// Loopback address with an OS-assigned ephemeral port, so concurrent tests never collide.
pub fn loopback_addr() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}

/// Installs the process-wide rustls crypto provider (idempotent). Every test that touches rustls
/// must call this: under cargo-nextest each test runs in its own process, so the provider may not
/// be installed, while `cargo test` shares one process and masks the gap.
pub fn ensure_crypto_provider() {
    let _ = tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();
}
