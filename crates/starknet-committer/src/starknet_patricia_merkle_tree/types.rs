use std::collections::HashMap;

use committer::patricia_merkle_tree::filled_tree::errors::FilledTreeError;
use committer::patricia_merkle_tree::filled_tree::tree::FilledTreeImpl;

use crate::block_committer::input::{ContractAddress, StarknetStorageValue};
use crate::starknet_patricia_merkle_tree::node::CompiledClassHash;
use crate::starknet_patricia_merkle_tree::starknet_leaf::leaf::ContractState;

pub type StorageTrie = FilledTreeImpl<StarknetStorageValue>;
pub type ClassesTrie = FilledTreeImpl<CompiledClassHash>;
pub type ContractsTrie = FilledTreeImpl<ContractState>;
pub type StorageTrieMap = HashMap<ContractAddress, StorageTrie>;

pub type StorageTrieError = FilledTreeError<StarknetStorageValue>;
pub type ClassesTrieError = FilledTreeError<CompiledClassHash>;
pub type ContractsTrieError = FilledTreeError<ContractState>;

#[cfg(test)]
pub mod types_test {
    use committer::felt::Felt;
    use committer::patricia_merkle_tree::types::NodeIndex;
    use rstest::rstest;

    use crate::block_committer::input::{ContractAddress, StarknetStorageKey};

    #[rstest]
    fn test_cast_to_node_index(
        #[values(0, 15, 0xDEADBEEF)] leaf_index: u128,
        #[values(true, false)] bool_from_contract_address: bool,
    ) {
        let expected_node_index = NodeIndex::FIRST_LEAF + leaf_index;
        let actual: NodeIndex = if bool_from_contract_address {
            (&ContractAddress(Felt::from(leaf_index))).into()
        } else {
            (&StarknetStorageKey(Felt::from(leaf_index))).into()
        };
        assert_eq!(actual, expected_node_index);
    }
}
