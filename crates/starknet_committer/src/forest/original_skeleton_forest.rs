use std::collections::HashMap;

use starknet_api::core::ContractAddress;
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTreeImpl;
use starknet_patricia::patricia_merkle_tree::types::SortedLeafIndices;

#[derive(Debug, PartialEq)]
pub struct OriginalSkeletonForest<'a> {
    pub(crate) classes_trie: OriginalSkeletonTreeImpl<'a>,
    pub(crate) contracts_trie: OriginalSkeletonTreeImpl<'a>,
    pub(crate) storage_tries: HashMap<ContractAddress, OriginalSkeletonTreeImpl<'a>>,
}

/// Holds all the indices of the modified leaves in the Starknet forest grouped by tree and sorted.
pub struct ForestSortedIndices<'a> {
    pub(crate) storage_tries_sorted_indices: HashMap<ContractAddress, SortedLeafIndices<'a>>,
    pub(crate) contracts_trie_sorted_indices: SortedLeafIndices<'a>,
    pub(crate) classes_trie_sorted_indices: SortedLeafIndices<'a>,
}
