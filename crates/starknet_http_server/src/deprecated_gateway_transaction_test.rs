use assert_matches::assert_matches;
use starknet_api::compression_utils::CompressionError;
use starknet_api::rpc_transaction::RpcDeclareTransactionV3;
use starknet_api::test_utils::read_json_file;

use crate::deprecated_gateway_transaction::{
    DeprecatedGatewayDeclareTransaction,
    DeprecatedGatewayDeployAccountTransaction,
    DeprecatedGatewayInvokeTransaction,
};

// Utils.

const DEPRECATED_GATEWAY_INVOKE_TX_JSON_PATH: &str = "deprecated_gateway/invoke_tx.json";
const DEPRECATED_GATEWAY_DEPLOY_ACCOUNT_TX_JSON_PATH: &str =
    "deprecated_gateway/deploy_account_tx.json";
const DEPRECATED_GATEWAY_DECLARE_TX_JSON_PATH: &str = "deprecated_gateway/declare_tx.json";

fn deprecated_gateway_declare_tx() -> DeprecatedGatewayDeclareTransaction {
    serde_json::from_value(read_json_file(DEPRECATED_GATEWAY_DECLARE_TX_JSON_PATH))
        .expect("Failed to deserialize json to RestDeclareTransactionV3")
}

// Tests.

#[test]
fn deprecated_gateway_invoke_tx_deserialization() {
    let _: DeprecatedGatewayInvokeTransaction =
        serde_json::from_value(read_json_file(DEPRECATED_GATEWAY_INVOKE_TX_JSON_PATH))
            .expect("Failed to deserialize json to RestInvokeTransactionV3");
}

#[test]
fn deprecated_gateway_deploy_account_tx_deserialization() {
    let _: DeprecatedGatewayDeployAccountTransaction =
        serde_json::from_value(read_json_file(DEPRECATED_GATEWAY_DEPLOY_ACCOUNT_TX_JSON_PATH))
            .expect("Failed to deserialize json to RestDeployAccountTransactionV3");
}

#[test]
fn deprecated_gateway_declare_tx_conversion() {
    let deprecate_tx = deprecated_gateway_declare_tx();
    let deprecate_declare_tx = assert_matches!(
        deprecate_tx,
        DeprecatedGatewayDeclareTransaction::V3(deprecated_declare_tx) =>
        deprecated_declare_tx
    );
    // TODO(Arni): Assert the deprecated transaction was converted to the expected RPC transaction.
    let _declare_tx: RpcDeclareTransactionV3 = deprecate_declare_tx.try_into().unwrap();
}

#[test]
fn deprecated_gateway_declare_tx_negative_flow_conversion() {
    let deprecate_tx = deprecated_gateway_declare_tx();
    let mut deprecate_declare_tx = assert_matches!(
        deprecate_tx,
        DeprecatedGatewayDeclareTransaction::V3(deprecated_declare_tx) =>
        deprecated_declare_tx
    );

    deprecate_declare_tx.contract_class.sierra_program += "arbitrary";
    let error = RpcDeclareTransactionV3::try_from(deprecate_declare_tx).unwrap_err();
    assert_matches!(error, CompressionError::Decode(base64::DecodeError::InvalidLength));
}
