use std::collections::HashMap;

use starknet_api::state::StorageKey;
use starknet_committer::block_committer::commit::commit_block;
use starknet_committer::block_committer::input::{
    ConfigImpl,
    Input,
    StarknetStorageKey,
    StarknetStorageValue,
    StateDiff,
};
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia_storage::storage_trait::{
    BlockNumber,
    DbKey,
    DbValue,
    KeyContext,
    Storage,
    TrieKey,
    TrieType,
};
pub type InputImpl = Input<ConfigImpl>;

use rand::rngs::SmallRng;
use rand::SeedableRng;
use starknet_api::core::{ascii_as_felt, ContractAddress};
use starknet_committer::block_committer::state_diff_generator::RANDOM_STATE_DIFF_CONTRACT_ADDRESS;
use starknet_patricia_storage::map_storage::{MapStorage, MapStorageLayer};
use starknet_types_core::felt::Felt;

use crate::commands::BenchmarkFlavor;

const PREFIX_BYTES_LENGTH: usize = 32;

fn extract_prefix(key: &DbKey) -> Vec<u8> {
    key.0.iter().take(PREFIX_BYTES_LENGTH).copied().collect()
}

const NUM_BLOCKS: u64 = 2;

fn get_expected_prefixes() -> Vec<Vec<u8>> {
    vec![
        ascii_as_felt("CONTRACTS_TREE_PREFIX").unwrap().to_bytes_be().to_vec(),
        ascii_as_felt("CONTRACTS_TREE_PREFIX").unwrap().to_bytes_be().to_vec(),
        Felt::from(RANDOM_STATE_DIFF_CONTRACT_ADDRESS).to_bytes_be().to_vec(),
    ]
}

#[tokio::test]
async fn test_storage_access_by_node_index() {
    let expected_prefixes = get_expected_prefixes();

    let flavor = BenchmarkFlavor::Constant1KDiff;
    let mut storage = MapStorage::new();

    let classes_trie_root_hash: HashOutput = HashOutput::default();
    let contracts_trie_root_hash: HashOutput = HashOutput::default();

    for block_number in 0..NUM_BLOCKS {
        let mut rng = SmallRng::seed_from_u64(42 + block_number);
        let input = InputImpl {
            state_diff: flavor.generate_state_diff(1, &mut rng),
            contracts_trie_root_hash,
            classes_trie_root_hash,
            config: ConfigImpl::default(),
        };

        // read from latest trie
        let filled_forest = commit_block(input, &mut storage, None)
            .await
            .expect("Failed to commit the given block.");

        // write to latest trie
        filled_forest.write_to_storage(&mut storage, None);
        // write to historical trie
        filled_forest.write_to_storage(&mut storage, Some(BlockNumber(block_number)));
    }

    let actual_keys = storage.0.get(&MapStorageLayer::LatestTrie).unwrap().keys();
    for key in actual_keys {
        let prefix = extract_prefix(key.into());
        assert!(expected_prefixes.contains(&prefix));
    }
}

#[tokio::test]
async fn test_historical_access() {
    let classes_trie_root_hash: HashOutput = HashOutput::default();
    let contracts_trie_root_hash: HashOutput = HashOutput::default();

    let mut storage = MapStorage::new();
    let mut diff1 = StateDiff::default();
    let mut storage_diffs_1: HashMap<StarknetStorageKey, StarknetStorageValue> = HashMap::new();
    storage_diffs_1.insert(
        StarknetStorageKey(StorageKey::from(1_u128)),
        StarknetStorageValue(Felt::from_hex_unchecked("1")),
    );
    diff1.storage_updates.insert(ContractAddress::from(1_u128), storage_diffs_1);

    let input1 = InputImpl {
        state_diff: diff1,
        contracts_trie_root_hash,
        classes_trie_root_hash,
        config: ConfigImpl::default(),
    };

    let filled_forest =
        commit_block(input1, &mut storage, None).await.expect("Failed to commit the given block.");

    // write to latest trie
    filled_forest.write_to_storage(&mut storage, None);
    // write to historical trie
    filled_forest.write_to_storage(&mut storage, Some(BlockNumber(0)));

    let mut diff2 = StateDiff::default();
    let mut storage_diffs_2 = HashMap::new();
    storage_diffs_2.insert(
        StarknetStorageKey(StorageKey::from(1_u128)),
        StarknetStorageValue(Felt::from_hex_unchecked("2")),
    );
    storage_diffs_2.insert(
        StarknetStorageKey(StorageKey::from(2_u128)),
        StarknetStorageValue(Felt::from_hex_unchecked("3")),
    );
    diff2.storage_updates.insert(ContractAddress::from(1_u128), storage_diffs_2);

    let input2 = InputImpl {
        state_diff: diff2,
        contracts_trie_root_hash,
        classes_trie_root_hash,
        config: ConfigImpl::default(),
    };

    let filled_forest =
        commit_block(input2, &mut storage, None).await.expect("Failed to commit the given block.");

    // write to latest trie
    filled_forest.write_to_storage(&mut storage, None);
    // write to historical trie
    filled_forest.write_to_storage(&mut storage, Some(BlockNumber(1)));

    let recent_key_context = KeyContext {
        trie_type: TrieType::StorageTrie(ContractAddress::from(1_u128).into()),
        block_number: None,
    };

    let leaf_index = NodeIndex::from_leaf_felt(&Felt::from_hex_unchecked("1"));
    let trie_key_recent =
        TrieKey::from_node_index_and_context(leaf_index.to_bytes(), &recent_key_context);

    let value_recent = storage.get(&trie_key_recent).unwrap();

    // Assert that the latest value is 2.
    assert_eq!(value_recent, Some(DbValue(Felt::from_hex_unchecked("2").to_bytes_be().to_vec())));

    let historicacl_key_context = KeyContext {
        trie_type: TrieType::StorageTrie(ContractAddress::from(1_u128).into()),
        block_number: Some(BlockNumber(0)),
    };

    let trie_key_historical =
        TrieKey::from_node_index_and_context(leaf_index.to_bytes(), &historicacl_key_context);

    let value_historical = storage.get(&trie_key_historical).unwrap();

    // Assert that at block 0 the value was 1.
    assert_eq!(
        value_historical,
        Some(DbValue(Felt::from_hex_unchecked("1").to_bytes_be().to_vec()))
    );
}
