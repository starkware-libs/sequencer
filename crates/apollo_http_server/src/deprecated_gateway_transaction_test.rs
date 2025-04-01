use std::io::Write;

use assert_matches::assert_matches;
use rstest::rstest;
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

enum CompressionErrorType {
    Io,
    Serde,
    Decode,
}

impl CompressionErrorType {
    fn assert_matches_error_type(&self, error: CompressionError) {
        match self {
            CompressionErrorType::Io => {
                assert_matches!(error, CompressionError::Io(..))
            }
            CompressionErrorType::Serde => {
                assert_matches!(error, CompressionError::Serde(..))
            }
            CompressionErrorType::Decode => {
                assert_matches!(error, CompressionError::Decode(base64::DecodeError::InvalidLength))
            }
        }
    }
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

fn create_malformed_sierra_program_for_serde_error() -> String {
    let invalid_json = b"arbitrary";
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(invalid_json).unwrap();
    let compressed = encoder.finish().unwrap();
    base64::encode(compressed)
}

#[rstest]
#[case::io_error(base64::encode("arbitrary"), CompressionErrorType::Io)]
#[case::serde_error(create_malformed_sierra_program_for_serde_error(), CompressionErrorType::Serde)]
#[case::decode_error("arbitrary".to_string(), CompressionErrorType::Decode)]
fn deprecated_gateway_declare_tx_negative_flow_conversion(
    #[case] sierra_program: String,
    #[case] expected_error_type: CompressionErrorType,
) {
    let deprecate_tx = deprecated_gateway_declare_tx();
    let mut deprecate_declare_tx = assert_matches!(
        deprecate_tx,
        DeprecatedGatewayDeclareTransaction::V3(deprecated_declare_tx) =>
        deprecated_declare_tx
    );

    deprecate_declare_tx.contract_class.sierra_program = sierra_program;
    expected_error_type.assert_matches_error_type(
        RpcDeclareTransactionV3::try_from(deprecate_declare_tx).unwrap_err(),
    );
}
