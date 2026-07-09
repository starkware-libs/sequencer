//! Shared helpers for the server test modules.

use std::net::SocketAddr;

use bytes::Bytes;
use http::{HeaderMap, Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use jsonrpsee::server::HttpBody;
use jsonrpsee::RpcModule;
use serde_json::{json, Value};

use crate::server::mock_rpc::MockProvingRpc;
use crate::server::request_log::REQUEST_ID_HEADER;
use crate::server::rpc_api::ProvingRpcServer;

/// Connection cap for test servers; ample for the handful of requests a test makes.
pub const TEST_MAX_CONNECTIONS: u32 = 10;

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

/// A JSON-RPC 2.0 request envelope with a fixed id. `params` is omitted when `Value::Null`, since
/// the server distinguishes an absent `params` from an explicit null one.
pub fn jsonrpc_request(method: &str, params: Value) -> Value {
    let mut request = json!({ "jsonrpc": "2.0", "id": "1", "method": method });
    if !params.is_null() {
        request["params"] = params;
    }
    request
}

/// Echoes the request's `x-request-id` into the response body, so tests can observe the id
/// downstream layers saw.
pub fn echo_request_id_service() -> impl tower::Service<
    Request<HttpBody>,
    Response = Response<HttpBody>,
    Error = std::convert::Infallible,
    Future = futures::future::Ready<Result<Response<HttpBody>, std::convert::Infallible>>,
> + Clone {
    tower::service_fn(|request: Request<HttpBody>| {
        let id = request
            .headers()
            .get(REQUEST_ID_HEADER)
            .map(|value| value.to_str().expect("test ids are ASCII").to_string())
            .unwrap_or_default();
        let response = Response::builder()
            .status(StatusCode::OK)
            .body(HttpBody::new(Full::new(Bytes::from(id))))
            .expect("static body is infallible");
        futures::future::ready(Ok::<_, std::convert::Infallible>(response))
    })
}

pub async fn read_body_and_headers(response: Response<HttpBody>) -> (String, HeaderMap) {
    let (parts, body) = response.into_parts();
    let bytes = body.collect().await.expect("body collect").to_bytes();
    (String::from_utf8(bytes.to_vec()).expect("utf8 body"), parts.headers)
}
