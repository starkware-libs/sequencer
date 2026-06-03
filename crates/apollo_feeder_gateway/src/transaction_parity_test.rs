use apollo_starknet_client::reader::objects::transaction::Transaction;
use rstest::rstest;

use crate::serialization::to_python_json;

/// Reads a live-captured parity fixture: the EXACT bytes the Python feeder gateway served for one
/// transaction object (cut from real get_block responses on 2026-06-03; see each file's source
/// network/block in the PR that captured it).
fn read_transaction_parity_fixture(fixture_name: &str) -> String {
    let path =
        format!("{}/resources/parity/transactions/{fixture_name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("reading {path}: {error}"))
}

/// THE transaction byte-parity lock: deserializing a live transaction and re-serializing it
/// through the feeder gateway formatter must reproduce the Python feeder gateway's bytes exactly,
/// for every transaction family and version (key order, key names, value formats, separators).
#[rstest]
#[case::declare_v0("declare_v0.json")]
#[case::declare_v1("declare_v1.json")]
#[case::declare_v2("declare_v2.json")]
#[case::declare_v3("declare_v3.json")]
#[case::deploy_v0("deploy_v0.json")]
#[case::deploy_account_v1("deploy_account_v1.json")]
#[case::deploy_account_v3("deploy_account_v3.json")]
#[case::invoke_v0("invoke_v0.json")]
#[case::invoke_v1("invoke_v1.json")]
#[case::invoke_v3("invoke_v3.json")]
#[case::l1_handler_v0("l1_handler_v0.json")]
fn transaction_round_trip_is_byte_identical_to_live_bytes(#[case] fixture_name: &str) {
    let live_bytes = read_transaction_parity_fixture(fixture_name);
    let transaction: Transaction = serde_json::from_str(&live_bytes)
        .unwrap_or_else(|error| panic!("deserializing {fixture_name}: {error}"));
    assert_eq!(to_python_json(&transaction).unwrap(), live_bytes, "drift in {fixture_name}");
}
