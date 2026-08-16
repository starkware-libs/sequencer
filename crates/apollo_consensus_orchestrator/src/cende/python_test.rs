//! Support for the `starknet_committer_and_os_cli` Python-compatibility tests.

use std::fmt::Debug;

use blockifier::state::accessed_keys::AccessedKeys;
use blockifier::state::cached_state::CommitmentStateDiff;
use indexmap::indexmap;
use serde::Deserialize;
use shared_execution_objects::central_objects::CentralTransactionExecutionInfo;
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::state::StorageKey;
use starknet_api::transaction::fields::ProofFacts;
use starknet_api::{contract_address, felt, nonce};

use super::BlockAccessedKeysData;

#[cfg(test)]
#[path = "python_test_test.rs"]
mod python_test_test;

/// The central-object blob fixture shared with the Python repo: the Python-side test writes it
/// through the recorder and sends the `get_accessed_keys_input` response back here.
const CENTRAL_BLOB_JSON: &str = include_str!("../../resources/central_blob.json");

/// The accessed keys the fixture computes to with `accessed_keys_state_diff`. Pinned so that any
/// serialization or computation change fails the test.
const CENTRAL_BLOB_ACCESSED_KEYS_JSON: &str =
    include_str!("../../resources/central_blob_accessed_keys.json");

/// The blob fields the payload is checked against.
#[derive(Deserialize)]
struct CentralBlobFixture {
    transactions: Vec<CentralBlobTransactionFixture>,
    execution_infos: Vec<CentralTransactionExecutionInfo>,
}

/// A blob `transactions` entry, narrowed to its proof facts.
#[derive(Deserialize)]
struct CentralBlobTransactionFixture {
    tx: CentralBlobTransactionProofFacts,
}

/// The blob omits `proof_facts` when a transaction carries none; the payload must send an empty
/// entry for such a transaction.
#[derive(Deserialize)]
struct CentralBlobTransactionProofFacts {
    #[serde(default)]
    proof_facts: ProofFacts,
}

/// Checks the recorder's `get_accessed_keys_input` payload against the fixtures: it must carry the
/// blob's proof facts and execution infos, and they must compute to the expected accessed keys.
/// Returns "Success", or a description of the first mismatch.
pub fn parse_accessed_keys_input_test(input: &str) -> Result<String, serde_json::Error> {
    let actual_accessed_keys_data: BlockAccessedKeysData = serde_json::from_str(input)?;
    let expected_central_blob: CentralBlobFixture = serde_json::from_str(CENTRAL_BLOB_JSON)?;
    let expected_accessed_keys: AccessedKeys =
        serde_json::from_str(CENTRAL_BLOB_ACCESSED_KEYS_JSON)?;

    let expected_proof_facts: Vec<ProofFacts> = expected_central_blob
        .transactions
        .iter()
        .map(|transaction| transaction.tx.proof_facts.clone())
        .collect();

    if actual_accessed_keys_data.proof_facts != expected_proof_facts {
        return Ok(mismatch(
            "proof_facts",
            &expected_proof_facts,
            &actual_accessed_keys_data.proof_facts,
        ));
    }
    if actual_accessed_keys_data.execution_infos != expected_central_blob.execution_infos {
        return Ok(mismatch(
            "execution_infos",
            &expected_central_blob.execution_infos,
            &actual_accessed_keys_data.execution_infos,
        ));
    }
    let actual_accessed_keys =
        actual_accessed_keys_data.compute_accessed_keys(&accessed_keys_state_diff());
    if actual_accessed_keys != expected_accessed_keys {
        return Ok(mismatch("accessed_keys", &expected_accessed_keys, &actual_accessed_keys));
    }
    Ok("Success".to_string())
}

fn mismatch<T: Debug>(field_name: &str, expected: &T, actual: &T) -> String {
    format!("Failure: {field_name} mismatch.\nexpected: {expected:?}\nactual: {actual:?}")
}

/// A fixed state diff standing in for the synced block's state diff (an input the node already
/// holds, not part of the recorder payload). Covers every state-diff contribution to the accessed
/// keys; the storage key is high enough to get an alias under stateful compression.
fn accessed_keys_state_diff() -> CommitmentStateDiff {
    CommitmentStateDiff {
        address_to_class_hash: indexmap!(
            contract_address!("0x1001") => ClassHash(felt!("0x2002"))
        ),
        address_to_nonce: indexmap!(contract_address!("0x1001") => nonce!(1)),
        storage_updates: indexmap!(
            contract_address!("0x1003") => indexmap!(StorageKey::from(0x700_u128) => felt!("0x8"))
        ),
        class_hash_to_compiled_class_hash: indexmap!(
            ClassHash(felt!("0x2004")) => CompiledClassHash(felt!("0x2005"))
        ),
    }
}
