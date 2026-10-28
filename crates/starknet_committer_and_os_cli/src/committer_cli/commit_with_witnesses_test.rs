use apollo_consensus_orchestrator::cende::BlockAccessedKeysData;
use blockifier::state::cached_state::CommitmentStateDiff;
use indexmap::indexmap;
use starknet_api::core::BLOCK_HASH_TABLE_ADDRESS;
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::fields::{
    PROOF_VERSION_V0,
    VIRTUAL_OS_OUTPUT_VERSION,
    VIRTUAL_SNOS,
};
use starknet_api::{class_hash, compiled_class_hash, contract_address, felt, nonce, storage_key};
use starknet_committer::block_committer::in_memory_commit::{
    commit_state_diff_to_index_db,
    commit_state_diff_with_witnesses,
};
use starknet_committer::db::forest_trait::StorageInitializer;
use starknet_committer::db::index_db::IndexDb;
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_types_core::felt::Felt;

use super::{commit_with_witnesses, CommitWithWitnessesInput};

/// Three blocks: the contract is deployed and its cell written, the cell is rewritten, and the
/// block under test touches its nonce and cell while a proof-facts transaction reads block 7's
/// hash from the block hash table.
#[tokio::test]
async fn output_matches_direct_commit_and_holds_pre_block_values() {
    let address = contract_address!("0x1000");
    let class_hash = class_hash!("0x2000");
    let key = storage_key!("0x300");
    let committed_state_diffs = vec![
        ThinStateDiff {
            deployed_contracts: indexmap! { address => class_hash },
            storage_diffs: indexmap! { address => indexmap! { key => felt!("0x1") } },
            class_hash_to_compiled_class_hash: indexmap! { class_hash => compiled_class_hash!(0x2001_u32) },
            nonces: indexmap! { address => nonce!(1) },
            ..Default::default()
        },
        ThinStateDiff {
            storage_diffs: indexmap! { address => indexmap! { key => felt!("0x2") } },
            ..Default::default()
        },
    ];
    let state_diff = ThinStateDiff {
        storage_diffs: indexmap! { address => indexmap! { key => felt!("0x3") } },
        nonces: indexmap! { address => nonce!(2) },
        ..Default::default()
    };
    let proof_facts_block_number = 7_u64;
    let proof_facts = [
        PROOF_VERSION_V0,
        VIRTUAL_SNOS,
        felt!("0x123"), // Program hash.
        VIRTUAL_OS_OUTPUT_VERSION,
        Felt::from(proof_facts_block_number),
        felt!("0x456"), // Block hash.
        felt!("0x789"), // Config hash.
    ];
    let accessed_keys_input: BlockAccessedKeysData = serde_json::from_value(serde_json::json!({
        "proof_facts": [proof_facts],
        "execution_infos": [],
    }))
    .unwrap();
    let expected_accessed_keys =
        accessed_keys_input.compute_accessed_keys(&CommitmentStateDiff::from(state_diff.clone()));

    let output = commit_with_witnesses(CommitWithWitnessesInput {
        committed_state_diffs: committed_state_diffs.clone(),
        state_diff: state_diff.clone(),
        accessed_keys_input,
    })
    .await
    .unwrap();

    // The same chain committed directly through the production witness path.
    let mut index_db = IndexDb::new(MapStorage::default());
    for committed_state_diff in committed_state_diffs {
        commit_state_diff_to_index_db(&mut index_db, committed_state_diff.into()).await.unwrap();
    }
    let (expected_roots, expected_state_commitment_infos) =
        commit_state_diff_with_witnesses(&mut index_db, state_diff.into(), &expected_accessed_keys)
            .await
            .unwrap();
    assert_eq!(output.contracts_trie_root_hash, expected_roots.contracts_trie_root_hash);
    assert_eq!(output.classes_trie_root_hash, expected_roots.classes_trie_root_hash);
    assert_eq!(
        output.state_commitment_infos.decompress().unwrap(),
        expected_state_commitment_infos
    );

    // Pre-block values: the last committed write, the committed nonce and class hash, and zero for
    // the never-written block hash table cell the proof facts reference.
    let initial_reads = &output.initial_reads;
    assert_eq!(initial_reads.storage[&(address, key)], felt!("0x2"));
    assert_eq!(initial_reads.nonces[&address], nonce!(1));
    assert_eq!(initial_reads.class_hashes[&address], class_hash);
    let block_hash_cell = (BLOCK_HASH_TABLE_ADDRESS, storage_key!(proof_facts_block_number));
    assert!(expected_accessed_keys.storage_keys.contains(&block_hash_cell));
    assert_eq!(initial_reads.storage[&block_hash_cell], Felt::ZERO);
    assert!(initial_reads.declared_contracts.is_empty());
    // Every key the witnesses cover has a value.
    assert!(
        expected_accessed_keys.storage_keys.iter().all(|k| initial_reads.storage.contains_key(k))
    );
    assert!(expected_accessed_keys.accessed_contracts.iter().all(|a| {
        initial_reads.nonces.contains_key(a) && initial_reads.class_hashes.contains_key(a)
    }));
    assert!(
        expected_accessed_keys
            .accessed_class_hashes
            .iter()
            .all(|c| initial_reads.compiled_class_hashes.contains_key(c))
    );

    // The blob encodings: the compressed payload entry and the nested initial reads.
    let json = serde_json::to_value(&output).unwrap();
    assert_eq!(json["state_commitment_infos"]["version"], 0);
    assert!(json["state_commitment_infos"]["payload"].is_string());
    assert_eq!(
        json["initial_reads"]["storage"][address.to_hex_string()][key.to_hex_string()],
        "0x2"
    );
    assert_eq!(json["initial_reads"]["nonces"][address.to_hex_string()], "0x1");
}
