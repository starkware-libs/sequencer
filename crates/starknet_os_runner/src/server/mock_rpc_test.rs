//! Serde roundtrip tests for the ProvingRpc JSON fixtures.
//!
//! These tests verify that the fixture JSON files used by [`MockProvingRpc`] remain
//! compatible with the Rust types. If someone changes the serde layout of `BlockId`,
//! `RpcTransaction`, or `ProveTransactionResult`, these tests will fail — signalling
//! that the SDK's expected JSON format has drifted.

use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use starknet_api::rpc_transaction::RpcTransaction;

use crate::proving::virtual_snos_prover::ProveTransactionResult;
use crate::server::mock_rpc::{load_fixture_params, load_fixture_response};

/// Verifies the request-params fixture deserializes into `BlockId` + `RpcTransaction`
/// and that re-serialization produces equivalent JSON.
#[test]
fn test_prove_transaction_params_json_format() {
    let params = load_fixture_params();

    // Deserialize each field from the fixture.
    let block_id: BlockId = serde_json::from_value(params["block_id"].clone())
        .expect("Failed to deserialize block_id from fixture");
    let transaction: RpcTransaction = serde_json::from_value(params["transaction"].clone())
        .expect("Failed to deserialize transaction from fixture");

    // Round-trip: re-serialize and compare.
    let block_id_roundtrip = serde_json::to_value(&block_id).unwrap();
    let tx_roundtrip = serde_json::to_value(&transaction).unwrap();

    assert_eq!(
        params["block_id"], block_id_roundtrip,
        "block_id serde round-trip mismatch — the BlockId JSON format may have changed"
    );
    assert_eq!(
        params["transaction"], tx_roundtrip,
        "transaction serde round-trip mismatch — the RpcTransaction JSON format may have changed"
    );
}

/// Verifies the response fixture deserializes into `ProveTransactionResult` and that
/// re-serialization produces identical JSON.
#[test]
fn test_prove_transaction_result_json_format() {
    // load_fixture_response already asserts deserialization succeeds.
    let result: ProveTransactionResult = load_fixture_response();

    // Round-trip: serialize → deserialize → serialize, then compare.
    let json = serde_json::to_value(&result).unwrap();
    let roundtripped: ProveTransactionResult = serde_json::from_value(json.clone()).unwrap();
    let json2 = serde_json::to_value(&roundtripped).unwrap();

    assert_eq!(json, json2, "ProveTransactionResult serde round-trip mismatch");

    // Compare against the raw fixture file to catch normalization differences.
    let fixture_json: serde_json::Value = {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources/mock_proving_rpc/prove_transaction_result.json");
        let content = std::fs::read_to_string(&path).unwrap();
        serde_json::from_str(&content).unwrap()
    };
    assert_eq!(
        fixture_json, json,
        "Response fixture does not match re-serialized ProveTransactionResult"
    );
}
