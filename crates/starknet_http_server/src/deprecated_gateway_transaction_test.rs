use starknet_api::test_utils::read_json_file;

use crate::deprecated_gateway_transaction::{
    DeprecatedGatewayDeclareTransaction,
    DeprecatedGatewayDeployAccountTransaction,
    DeprecatedGatewayInvokeTransaction,
};

const DEPRECATED_GATEWAY_INVOKE_TX_JSON_PATH: &str = "deprecated_gateway/invoke_tx.json";
const DEPRECATED_GATEWAY_DEPLOY_ACCOUNT_TX_JSON_PATH: &str =
    "deprecated_gateway/deploy_account_tx.json";
const DEPRECATED_GATEWAY_DECLARE_TX_JSON_PATH: &str = "deprecated_gateway/declare_tx.json";

#[test]
fn rest_invoke_tx_deserialization() {
    let _: DeprecatedGatewayInvokeTransaction =
        serde_json::from_value(read_json_file(DEPRECATED_GATEWAY_INVOKE_TX_JSON_PATH))
            .expect("Failed to deserialize json to RestInvokeTransactionV3");
}

#[test]
fn rest_deploy_account_tx_deserialization() {
    let _: DeprecatedGatewayDeployAccountTransaction =
        serde_json::from_value(read_json_file(DEPRECATED_GATEWAY_DEPLOY_ACCOUNT_TX_JSON_PATH))
            .expect("Failed to deserialize json to RestDeployAccountTransactionV3");
}

#[test]
fn rest_declare_tx_deserialization() {
    let _: DeprecatedGatewayDeclareTransaction =
        serde_json::from_value(read_json_file(DEPRECATED_GATEWAY_DECLARE_TX_JSON_PATH))
            .expect("Failed to deserialize json to RestDeclareTransactionV3");
}
