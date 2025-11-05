#[cfg(test)]
mod tests {
    use starknet_patricia::hash::hash_trait::HashOutput;
    use starknet_patricia_storage::storage_trait::DbKey;
    use starknet_committer::block_committer::commit::commit_block;
    use starknet_committer::block_committer::input::{
        ConfigImpl,
        Input
    };

    pub type InputImpl = Input<ConfigImpl>;


    use rand::rngs::SmallRng;
    use rand::SeedableRng;
    use starknet_types_core::felt::Felt;
    use crate::commands::BenchmarkFlavor;

    use starknet_committer::block_committer::state_diff_generator::RANDOM_STATE_DIFF_CONTRACT_ADDRESS;

    use starknet_api::core::ascii_as_felt;

    use starknet_patricia_storage::map_storage::MapStorage;

    const PREFIX_BYTES_LENGTH: usize = 32;

    fn extract_prefix(key: &DbKey) -> Vec<u8> {
        key.0.iter().take(PREFIX_BYTES_LENGTH).copied().collect()
    }

    const NUM_BLOCKS: u64 = 2;

    fn get_expected_prefixes() -> Vec<Vec<u8>> {
        vec![
            ascii_as_felt("CONTRACTS_TREE_PREFIX").unwrap().to_bytes_be().to_vec(),
            ascii_as_felt("CONTRACTS_TREE_PREFIX").unwrap().to_bytes_be().to_vec(),
            Felt::from(RANDOM_STATE_DIFF_CONTRACT_ADDRESS).to_bytes_be().to_vec()
        ]
    }

    #[tokio::test]
    async fn test_storage_access_by_node_index() {
        let expected_prefixes = get_expected_prefixes();

        let flavor = BenchmarkFlavor::Constant1KDiff;
        //let mut storage_mock = MockStorage::new();
        let mut storage = MapStorage::default();

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

            let filled_forest = commit_block(input, &mut storage, None).await.expect("Failed to commit the given block.");
            filled_forest.write_to_storage(&mut storage);
        }

        let actual_keys = storage.0.keys();
        for key in actual_keys {
            let prefix = extract_prefix(key);
            assert!(expected_prefixes.contains(&prefix));
        }
    }
}