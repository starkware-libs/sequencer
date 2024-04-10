use axum::body::{Body, Bytes, HttpBody};
use axum::http::{Request, StatusCode};
use pretty_assertions::assert_str_eq;
use rstest::rstest;
use starknet_gateway::gateway::app;
use std::fs;
use tower::ServiceExt;

// TODO(Ayelet): Replace the use of the JSON files with generated instances, then serialize these
// into JSON for testing.
#[rstest]
#[case("./src/json_files_for_testing/declare_v3.json", "DECLARE")]
#[case(
    "./src/json_files_for_testing/deploy_account_v3.json",
    "DEPLOY_ACCOUNT"
)]
#[case("./src/json_files_for_testing/invoke_v3.json", "INVOKE")]
#[tokio::test]
async fn test_routes(#[case] json_file_path: &str, #[case] expected_response: &str) {
    let tx_json = fs::read_to_string(json_file_path).unwrap();
    let request = Request::post("/add_transaction")
        .header("content-type", "application/json")
        .body(Body::from(tx_json))
        .unwrap();

    let response = check_request(request, StatusCode::OK).await;

    assert_str_eq!(expected_response, String::from_utf8_lossy(&response));
}

#[tokio::test]
#[should_panic]
// FIXME: Currently is_alive is not implemented, fix this once it is implemented.
async fn test_is_alive() {
    let request = Request::get("/is_alive").body(Body::empty()).unwrap();
    // Status code doesn't matter, this panics ATM.
    check_request(request, StatusCode::default()).await;
}

async fn check_request(request: Request<Body>, status_code: StatusCode) -> Bytes {
    let response = app().oneshot(request).await.unwrap();
    assert_eq!(response.status(), status_code);

    response.into_body().collect().await.unwrap().to_bytes()
}
