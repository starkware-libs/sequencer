use mempool_test_utils::starknet_api_test_utils::{contract_class, COMPILED_CLASS_HASH};

use crate::compile::sierra_to_versioned_contract_class_v1;

/// Pins the in-process compilation output to the fixture's known compiled class hash — the same
/// hash the gateway's compilation flow produces for this class — guarding against output
/// divergence from the compiler used elsewhere in the system.
#[test]
fn compiled_fixture_class_matches_expected_compiled_class_hash() {
    let (contract_class, _sierra_version) =
        sierra_to_versioned_contract_class_v1(contract_class()).unwrap();

    assert_eq!(contract_class.compiled_class_hash(), *COMPILED_CLASS_HASH);
}
