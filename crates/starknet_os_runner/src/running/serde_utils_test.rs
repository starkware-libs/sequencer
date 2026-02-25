use rstest::rstest;
use serde_json::json;

use crate::running::serde_utils::deserialize_rpc_initial_reads;

/// Verifies that pathfinder v0.10 `initial_reads` JSON is correctly deserialized into
/// blockifier `StateMaps`.
///
/// JSON format matches pathfinder's `InitialReads` serialization:
/// https://github.com/eqlabs/pathfinder/blob/main/crates/rpc/src/dto/simulation.rs
#[rstest]
#[case::all_fields(
    json!({
        "storage": [{"contract_address": "0xabc", "key": "0x10", "value": "0x42"}],
        "nonces": [{"contract_address": "0xabc", "nonce": "0x5"}],
        "class_hashes": [{"contract_address": "0xabc", "class_hash": "0xdef"}],
        "declared_contracts": [{"class_hash": "0xdef", "is_declared": true}]
    }),
    1, 1, 1, 1
)]
#[case::empty_object(json!({}), 0, 0, 0, 0)]
#[case::only_nonces(
    json!({"nonces": [{"contract_address": "0x1", "nonce": "0x0"}]}),
    0, 1, 0, 0
)]
fn test_deserialize_rpc_initial_reads(
    #[case] input: serde_json::Value,
    #[case] expected_storage: usize,
    #[case] expected_nonces: usize,
    #[case] expected_class_hashes: usize,
    #[case] expected_declared_contracts: usize,
) {
    let state_maps = deserialize_rpc_initial_reads(input).unwrap();

    assert_eq!(state_maps.storage.len(), expected_storage);
    assert_eq!(state_maps.nonces.len(), expected_nonces);
    assert_eq!(state_maps.class_hashes.len(), expected_class_hashes);
    assert_eq!(state_maps.declared_contracts.len(), expected_declared_contracts);
    assert!(state_maps.compiled_class_hashes.is_empty());
}
