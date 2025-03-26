use starknet_api::test_utils::read_json_file;

use crate::rest_api_transaction::{
    RestDeclareTransactionV3,
    RestDeployAccountTransactionV3,
    RestInvokeTransactionV3,
};

const REST_INVOKE_TX_JSON_PATH: &str = "rest_api/invoke_tx.json";
const REST_DEPLOY_ACCOUNT_TX_JSON_PATH: &str = "rest_api/deploy_account_tx.json";
const REST_DECLARE_TX_JSON_PATH: &str = "rest_api/declare_tx.json";

#[test]
fn rest_invoke_tx_deserialization() {
    let _: RestInvokeTransactionV3 =
        serde_json::from_value(read_json_file(REST_INVOKE_TX_JSON_PATH))
            .expect("Failed to deserialize json to RestInvokeTransactionV3");
}

#[test]
fn rest_deploy_account_tx_deserialization() {
    let _: RestDeployAccountTransactionV3 =
        serde_json::from_value(read_json_file(REST_DEPLOY_ACCOUNT_TX_JSON_PATH))
            .expect("Failed to deserialize json to RestDeployAccountTransactionV3");
}

#[test]
fn rest_declare_tx_deserialization() {
    let _: RestDeclareTransactionV3 =
        serde_json::from_value(read_json_file(REST_DECLARE_TX_JSON_PATH))
            .expect("Failed to deserialize json to RestDeclareTransactionV3");
}
