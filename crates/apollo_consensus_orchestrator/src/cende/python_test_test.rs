use serde_json::json;

use super::{parse_accessed_keys_input_test, CENTRAL_BLOB_JSON};

/// Builds the `get_accessed_keys_input` payload the recorder would serve for the fixture blob: one
/// entry per transaction, holding the transaction's proof facts, or empty when the blob omits them.
fn fixture_payload() -> serde_json::Value {
    let blob: serde_json::Value = serde_json::from_str(CENTRAL_BLOB_JSON).unwrap();
    let proof_facts: Vec<serde_json::Value> = blob["transactions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|transaction| transaction["tx"].get("proof_facts").cloned().unwrap_or(json!([])))
        .collect();
    json!({
        "proof_facts": proof_facts,
        "execution_infos": blob["execution_infos"],
    })
}

#[test]
fn fixture_payload_matches_objects_and_accessed_keys() {
    let result = parse_accessed_keys_input_test(&fixture_payload().to_string()).unwrap();
    assert_eq!(result, "Success");
}

#[test]
fn missing_proof_facts_entry_reports_mismatch() {
    let mut payload = fixture_payload();
    payload["proof_facts"].as_array_mut().unwrap().pop().unwrap();

    let result = parse_accessed_keys_input_test(&payload.to_string()).unwrap();
    assert!(result.starts_with("Failure: proof_facts mismatch"), "unexpected result: {result}");
}

#[test]
fn modified_proof_facts_reports_mismatch() {
    let mut payload = fixture_payload();
    payload["proof_facts"][0].as_array_mut().unwrap().push(json!("0x1"));

    let result = parse_accessed_keys_input_test(&payload.to_string()).unwrap();
    assert!(result.starts_with("Failure: proof_facts mismatch"), "unexpected result: {result}");
}
