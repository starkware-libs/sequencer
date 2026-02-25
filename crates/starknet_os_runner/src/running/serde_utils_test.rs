use std::collections::HashMap;

use blockifier::state::cached_state::StateMaps;
use rstest::rstest;
use serde_json::json;
use starknet_api::{class_hash, contract_address, felt, nonce, storage_key};

use crate::running::serde_utils::deserialize_rpc_initial_reads;

/// Verifies that pathfinder v0.10 `initial_reads` JSON is correctly deserialized into
/// blockifier `StateMaps`.
///
/// JSON format matches pathfinder's `InitialReads` serialization:
/// https://github.com/eqlabs/pathfinder/blob/main/crates/rpc/src/dto/simulation.rs
#[rstest]
#[case::all_fields(
    json!({
        "storage": [
            {"contract_address": "0xabc", "key": "0x10", "value": "0x42"},
            {"contract_address": "0xabc", "key": "0x20", "value": "0x0"}
        ],
        "nonces": [{"contract_address": "0xabc", "nonce": "0x5"}],
        "class_hashes": [{"contract_address": "0xabc", "class_hash": "0xdef"}],
        "declared_contracts": [{"class_hash": "0xdef", "is_declared": true}]
    }),
    StateMaps {
        storage: HashMap::from([
            ((contract_address!("0xabc"), storage_key!("0x10")), felt!("0x42")),
            ((contract_address!("0xabc"), storage_key!("0x20")), felt!("0x0")),
        ]),
        nonces: HashMap::from([(contract_address!("0xabc"), nonce!(0x5_u64))]),
        class_hashes: HashMap::from([(contract_address!("0xabc"), class_hash!("0xdef"))]),
        declared_contracts: HashMap::from([(class_hash!("0xdef"), true)]),
        compiled_class_hashes: HashMap::default(),
    }
)]
#[case::empty_object(json!({}), StateMaps::default())]
#[case::only_nonces(
    json!({"nonces": [{"contract_address": "0x1", "nonce": "0x0"}]}),
    StateMaps {
        nonces: HashMap::from([(contract_address!("0x1"), nonce!(0_u64))]),
        ..StateMaps::default()
    }
)]
fn test_deserialize_rpc_initial_reads(
    #[case] input: serde_json::Value,
    #[case] expected: StateMaps,
) {
    let state_maps = deserialize_rpc_initial_reads(input).unwrap();
    assert_eq!(state_maps, expected);
}
