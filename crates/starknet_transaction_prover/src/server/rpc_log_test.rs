use bytes::Bytes;
use http::{header, Request};
use http_body_util::Full;
use jsonrpsee::server::{stop_channel, HttpBody, Methods, ServerBuilder};
use tower::{Layer, ServiceExt};
use tracing_test::traced_test;

use super::prover_rpc_middleware;
use crate::server::middleware_test_utils::read_body_and_headers;
use crate::server::mock_rpc::MockProvingRpc;
use crate::server::request_log::REQUEST_ID_HEADER;
use crate::server::request_span::RequestSpanLayer;
use crate::server::rpc_api::ProvingRpcServer;

const TEST_REQUEST_ID: &str = "rpc-log-test-id";

/// Sends `body` through jsonrpsee's HTTP service with the production RPC middleware, wrapped in
/// `RequestSpanLayer` as in production, and returns the response body. It runs on the test's own
/// task, so `traced_test` captures the lines it logs.
async fn send_json_rpc_body(body: &'static str) -> String {
    let methods: Methods = MockProvingRpc::from_expected_json().into_rpc().into();
    let (stop_handle, _server_handle) = stop_channel();
    let json_rpc_service = ServerBuilder::default()
        .set_rpc_middleware(prover_rpc_middleware(&methods))
        .to_service_builder()
        .build(methods, stop_handle);
    let request = Request::post("/")
        .header(header::CONTENT_TYPE, "application/json")
        .header(REQUEST_ID_HEADER, TEST_REQUEST_ID)
        .body(HttpBody::new(Full::new(Bytes::from_static(body.as_bytes()))))
        .expect("static request is valid");
    let response = RequestSpanLayer.layer(json_rpc_service).oneshot(request).await;
    read_body_and_headers(response.expect("jsonrpsee responds")).await.0
}

/// Checks that `RpcLogLayer` logged exactly one line, and that the line carries `latency_ms`, the
/// test's `request_id`, and every fragment in `expected_fragments`.
fn check_single_rpc_line(lines: &[&str], expected_fragments: &[&str]) -> Result<(), String> {
    let rpc_lines: Vec<&str> =
        lines.iter().copied().filter(|line| line.contains("event=\"rpc_")).collect();
    let [rpc_line] = rpc_lines[..] else {
        return Err(format!("expected one rpc line, got {rpc_lines:?}"));
    };
    let common_fragments = ["latency_ms=", TEST_REQUEST_ID];
    let mut required_fragments = common_fragments.iter().chain(expected_fragments);
    match required_fragments.find(|fragment| !rpc_line.contains(**fragment)) {
        Some(absent_fragment) => Err(format!("expected {absent_fragment:?} in {rpc_line:?}")),
        None => Ok(()),
    }
}

#[tokio::test]
#[traced_test]
async fn registered_method_is_logged_by_name() {
    send_json_rpc_body(r#"{"jsonrpc":"2.0","id":1,"method":"starknet_specVersion"}"#).await;

    let expected_fragments =
        ["event=\"rpc_request\"", "rpc_method=\"starknet_specVersion\"", "outcome=\"success\""];
    logs_assert(|lines: &[&str]| check_single_rpc_line(lines, &expected_fragments));
    assert!(!logs_contain("rpc_error_code"), "a successful call logs no error code");
}

#[tokio::test]
#[traced_test]
async fn unregistered_method_is_logged_as_unknown_without_params() {
    send_json_rpc_body(
        r#"{"jsonrpc":"2.0","id":1,"method":"client_chosen_name","params":["private-param"]}"#,
    )
    .await;

    let expected_fragments =
        ["rpc_method=\"unknown\"", "outcome=\"error\"", "rpc_error_code=-32601"];
    logs_assert(|lines: &[&str]| check_single_rpc_line(lines, &expected_fragments));
    assert!(!logs_contain("client_chosen_name"), "the logs must not carry the method name");
    assert!(!logs_contain("private-param"), "the logs must not carry params");
}

#[tokio::test]
#[traced_test]
async fn batch_is_logged_on_one_line() {
    let response_body = send_json_rpc_body(
        r#"[{"jsonrpc":"2.0","id":1,"method":"starknet_specVersion"},
            {"jsonrpc":"2.0","id":2,"method":"client_chosen_name"}]"#,
    )
    .await;

    let batch_responses: Vec<serde_json::Value> =
        serde_json::from_str(&response_body).expect("a batch gets a JSON array response");
    assert_eq!(batch_responses.len(), 2, "each call gets its own response, got {response_body}");
    let expected_fragments = ["event=\"rpc_batch\"", "batch_size=2"];
    logs_assert(|lines: &[&str]| check_single_rpc_line(lines, &expected_fragments));
}

#[tokio::test]
#[traced_test]
async fn notification_is_not_logged() {
    let response_body =
        send_json_rpc_body(r#"{"jsonrpc":"2.0","method":"starknet_specVersion"}"#).await;

    assert_eq!(response_body, "null", "jsonrpsee acknowledges a notification with a null body");
    assert!(!logs_contain("event=\"rpc_"), "a notification must not log a line");
}
