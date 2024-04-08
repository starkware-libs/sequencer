use crate::gateway::add_transaction;
use axum::{
    body::{Bytes, HttpBody},
    response::{IntoResponse, Response},
};
use pretty_assertions::assert_str_eq;
use rstest::rstest;
use starknet_api::external_transaction::ExternalTransaction;
use std::fs::File;

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
    let json_file = File::open(json_file_path).unwrap();
    let tx: ExternalTransaction = serde_json::from_reader(json_file).unwrap();

    let response = add_transaction(tx.into()).await.into_response();
    let response_bytes = &to_bytes(response).await;

    assert_str_eq!(&String::from_utf8_lossy(response_bytes), expected_response);
}

async fn to_bytes(res: Response) -> Bytes {
    res.into_body().collect().await.unwrap().to_bytes()
}
