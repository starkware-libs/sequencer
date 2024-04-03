use crate::gateway::add_transaction;
use axum::{body::HttpBody, response::IntoResponse};
use rstest::rstest;
use starknet_api::external_transaction::ExternalTransaction;
use std::fs::File;
use std::io::BufReader;

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
    let file = File::open(json_file_path).unwrap();
    let reader = BufReader::new(file);
    let transaction: ExternalTransaction = serde_json::from_reader(reader).unwrap();
    let response = add_transaction(transaction.into()).await.into_response();
    let response_bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        &String::from_utf8(response_bytes.to_vec()).unwrap(),
        expected_response
    );
}
