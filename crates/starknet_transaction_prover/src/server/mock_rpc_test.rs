//! Serde roundtrip test for the **response** format (`ProveTransactionResult`).
//!
//! Ensures the reference JSON file stays in sync with the Rust type.
//!
//! **If this test fails**:
//! 1. The serde layout of `ProveTransactionResult` has changed.
//! 2. Update `resources/mock_proving_rpc/prove_transaction_result.json` to match the new serialized
//!    format.
//! 3. Update the SDK to parse the new response format.

use crate::proving::virtual_snos_prover::ProveTransactionResult;

#[test]
fn test_prove_transaction_result_roundtrip() {
    let path = super::reference_json_dir().join("prove_transaction_result.json");

    let original_json: serde_json::Value = super::load_json(&path);
    let deserialized: ProveTransactionResult = super::load_json(&path);
    let roundtripped_json = serde_json::to_value(&deserialized).unwrap();

    assert_eq!(
        original_json, roundtripped_json,
        "Reference JSON differs from re-serialized ProveTransactionResult — format may have \
         drifted"
    );
}
