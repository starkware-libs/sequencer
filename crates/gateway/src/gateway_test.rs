use crate::gateway::add_transaction;
use crate::gateway::handle_request;
use hyper::{body, Body, Request};
use rstest::rstest;
use std::fs;

#[tokio::test]
async fn test_invalid_request() {
    // Create a sample GET request for an invalid path
    let request = Request::get("/some_invalid_path")
        .body(Body::empty())
        .unwrap();
    let response = handle_request(request).await.unwrap();

    assert_eq!(response.status(), 404);
    assert_eq!(
        String::from_utf8_lossy(&body::to_bytes(response.into_body()).await.unwrap()),
        "Not found."
    );
}

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
async fn test_add_transaction(#[case] json_file_path: &str, #[case] expected_response: &str) {
    let json_str = fs::read_to_string(json_file_path).expect("Failed to read JSON file");
    let body = Body::from(json_str);
    let response = add_transaction(body)
        .await
        .expect("Failed to process transaction");
    let bytes = body::to_bytes(response.into_body())
        .await
        .expect("Failed to read response body");
    let body_str = String::from_utf8(bytes.to_vec()).expect("Response body is not valid UTF-8");
    assert_eq!(body_str, expected_response);
}
