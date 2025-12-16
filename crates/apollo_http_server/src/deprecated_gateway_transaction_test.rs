use std::io::Write;

use apollo_http_server_config::config::DEFAULT_MAX_SIERRA_PROGRAM_SIZE;
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
    read_json_file(DEPRECATED_GATEWAY_DECLARE_TX_JSON_PATH)
}

// Tests.

#[test]
fn deprecated_gateway_invoke_tx_deserialization() {
    let _: DeprecatedGatewayInvokeTransaction =
        read_json_file(DEPRECATED_GATEWAY_INVOKE_TX_JSON_PATH);
}

#[test]
fn deprecated_gateway_deploy_account_tx_deserialization() {
    let _: DeprecatedGatewayDeployAccountTransaction =
        read_json_file(DEPRECATED_GATEWAY_DEPLOY_ACCOUNT_TX_JSON_PATH);
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
    let _declare_tx: RpcDeclareTransactionV3 =
        deprecate_declare_tx.convert_to_rpc_declare_tx(DEFAULT_MAX_SIERRA_PROGRAM_SIZE).unwrap();
}

fn create_malformed_sierra_program_for_serde_error() -> String {
    let invalid_json = b"arbitrary";
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(invalid_json).unwrap();
    let compressed = encoder.finish().unwrap();
    base64::encode(compressed)
}

#[rstest]
#[case::io_error(
    base64::encode("arbitrary"),
    |error| assert_matches!(error, CompressionError::Io(..))
)]
#[case::serde_error(
    create_malformed_sierra_program_for_serde_error(),
    |error| assert_matches!(error, CompressionError::Serde(..))
)]
#[case::decode_error(
    "arbitrary".to_string(),
    |error| assert_matches!(error, CompressionError::Decode(base64::DecodeError::InvalidLength))
)]
fn deprecated_gateway_declare_tx_negative_flow_conversion(
    #[case] sierra_program: String,
    #[case] assert_expected_error_fn: impl Fn(CompressionError),
) {
    let deprecate_tx = deprecated_gateway_declare_tx();
    let mut deprecate_declare_tx = assert_matches!(
        deprecate_tx,
        DeprecatedGatewayDeclareTransaction::V3(deprecated_declare_tx) =>
        deprecated_declare_tx
    );

    deprecate_declare_tx.contract_class.sierra_program = sierra_program;
    let error = deprecate_declare_tx
        .convert_to_rpc_declare_tx(DEFAULT_MAX_SIERRA_PROGRAM_SIZE)
        .unwrap_err();
    assert_expected_error_fn(error);
}

#[test]
fn test_bootstrap_declare_files_match() {
    use starknet_api::rpc_transaction::{RpcDeclareTransaction, RpcTransaction};

    println!("\n=== Comparing Bootstrap Declare Files ===\n");

    // Load the deprecated gateway bootstrap declare transaction
    let deprecated_bootstrap_tx: DeprecatedGatewayDeclareTransaction =
        read_json_file("bootstrap/deprecated_gateway/declare_tx_bootstrap_program.json");

    // Load the RPC format bootstrap declare transaction from integration tests
    let rpc_json_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("apollo_integration_tests/tests/test_data/bootstrap_declare.json");
    let rpc_bootstrap_tx: RpcTransaction = {
        let file = std::fs::File::open(&rpc_json_path)
            .unwrap_or_else(|e| panic!("Failed to open {:?}: {}", rpc_json_path, e));
        serde_json::from_reader(file)
            .unwrap_or_else(|e| panic!("Failed to parse {:?}: {}", rpc_json_path, e))
    };

    // Extract the RPC declare transaction V3
    let rpc_declare = match rpc_bootstrap_tx {
        RpcTransaction::Declare(RpcDeclareTransaction::V3(tx)) => tx,
        _ => panic!("Expected RpcTransaction::Declare::V3"),
    };

    // Try to convert deprecated to RPC format
    let conversion_result = match deprecated_bootstrap_tx {
        DeprecatedGatewayDeclareTransaction::V3(tx) => {
            tx.convert_to_rpc_declare_tx(DEFAULT_MAX_SIERRA_PROGRAM_SIZE)
        }
    };

    match conversion_result {
        Ok(deprecated_as_rpc) => {
            println!("✓ Successfully decompressed deprecated gateway sierra program");

            let mut all_match = true;

            // Compare sierra programs
            if deprecated_as_rpc.contract_class.sierra_program == rpc_declare.contract_class.sierra_program {
                println!("✓ Sierra programs match ({} elements)",
                    deprecated_as_rpc.contract_class.sierra_program.len());
            } else {
                println!("✗ Sierra programs DO NOT match");
                println!("  Deprecated format: {} elements", deprecated_as_rpc.contract_class.sierra_program.len());
                println!("  RPC format: {} elements", rpc_declare.contract_class.sierra_program.len());
                all_match = false;
            }

            // Compare other fields
            if deprecated_as_rpc.compiled_class_hash == rpc_declare.compiled_class_hash {
                println!("✓ Compiled class hashes match: {}", deprecated_as_rpc.compiled_class_hash);
            } else {
                println!("✗ Compiled class hashes DO NOT match");
                all_match = false;
            }

            if deprecated_as_rpc.sender_address == rpc_declare.sender_address {
                println!("✓ Sender addresses match: {}", deprecated_as_rpc.sender_address);
            } else {
                println!("✗ Sender addresses DO NOT match");
                all_match = false;
            }

            if deprecated_as_rpc.contract_class.entry_points_by_type == rpc_declare.contract_class.entry_points_by_type {
                println!("✓ Entry points match (EXTERNAL: {}, L1_HANDLER: {}, CONSTRUCTOR: {})",
                    deprecated_as_rpc.contract_class.entry_points_by_type.external.len(),
                    deprecated_as_rpc.contract_class.entry_points_by_type.l1handler.len(),
                    deprecated_as_rpc.contract_class.entry_points_by_type.constructor.len());
            } else {
                println!("✗ Entry points DO NOT match");
                all_match = false;
            }

            if deprecated_as_rpc.nonce == rpc_declare.nonce {
                println!("✓ Nonces match: {}", deprecated_as_rpc.nonce);
            } else {
                println!("✗ Nonces DO NOT match");
                all_match = false;
            }

            if deprecated_as_rpc.resource_bounds == rpc_declare.resource_bounds {
                println!("✓ Resource bounds match");
            } else {
                println!("✗ Resource bounds DO NOT match");
                all_match = false;
            }

            if all_match {
                println!("\n✓✓✓ RESULT: Both files represent the SAME transaction ✓✓✓\n");
            } else {
                println!("\n✗✗✗ RESULT: Files represent DIFFERENT transactions ✗✗✗\n");
                panic!("Bootstrap declare files do not match!");
            }
        }
        Err(e) => {
            println!("✗ FAILED to decompress deprecated gateway sierra program");
            println!("  Error: {:?}", e);
            println!("\n✗✗✗ RESULT: Cannot compare - deprecated file has INVALID/CORRUPTED sierra_program ✗✗✗\n");
            println!("The deprecated gateway file's sierra_program field appears to be:");
            println!("  - Not properly gzipped");
            println!("  - Corrupted during copy");
            println!("  - From a different/older version of the contract");
            panic!("Cannot convert deprecated bootstrap transaction: {:?}", e);
        }
    }
}
