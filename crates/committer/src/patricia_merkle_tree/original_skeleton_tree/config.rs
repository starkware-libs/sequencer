use crate::block_committer::input::StarknetStorageValue;
use crate::patricia_merkle_tree::filled_tree::node::CompiledClassHash;
use crate::patricia_merkle_tree::node_data::leaf::{ContractState, Leaf, LeafModifications};
use crate::patricia_merkle_tree::original_skeleton_tree::errors::OriginalSkeletonTreeError;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTreeResult;
use crate::patricia_merkle_tree::types::NodeIndex;

/// Configures the creation of an original skeleton tree.
pub trait OriginalSkeletonTreeConfig<L: Leaf> {
    /// Configures whether modified leaves should be compared to the previous leaves and log out a
    /// warning when encountering a trivial modification.
    fn compare_modified_leaves(&self) -> bool;

    /// Compares the previous leaf to the modified and returns true iff they are equal.
    fn compare_leaf(
        &self,
        index: &NodeIndex,
        previous_leaf: &L,
    ) -> OriginalSkeletonTreeResult<bool>;
}

#[macro_export]
macro_rules! generate_trie_config {
    ($struct_name:ident, $leaf_type:ty) => {
        pub struct $struct_name<'a> {
            modifications: &'a LeafModifications<$leaf_type>,
            compare_modified_leaves: bool,
        }

        impl<'a> $struct_name<'a> {
            #[allow(dead_code)]
            pub fn new(
                modifications: &'a LeafModifications<$leaf_type>,
                compare_modified_leaves: bool,
            ) -> Self {
                Self { modifications, compare_modified_leaves }
            }
        }

        impl OriginalSkeletonTreeConfig<$leaf_type> for $struct_name<'_> {
            fn compare_modified_leaves(&self) -> bool {
                self.compare_modified_leaves
            }

            fn compare_leaf(
                &self,
                index: &NodeIndex,
                previous_leaf: &$leaf_type,
            ) -> OriginalSkeletonTreeResult<bool> {
                let new_leaf = self
                    .modifications
                    .get(index)
                    .ok_or(OriginalSkeletonTreeError::ReadModificationsError(*index))?;
                Ok(new_leaf == previous_leaf)
            }
        }
    };
}

generate_trie_config!(OriginalSkeletonStorageTrieConfig, StarknetStorageValue);

generate_trie_config!(OriginalSkeletonClassesTrieConfig, CompiledClassHash);

pub(crate) struct OriginalSkeletonContractsTrieConfig;

impl OriginalSkeletonTreeConfig<ContractState> for OriginalSkeletonContractsTrieConfig {
    fn compare_modified_leaves(&self) -> bool {
        false
    }

    fn compare_leaf(
        &self,
        _index: &NodeIndex,
        _previous_leaf: &ContractState,
    ) -> OriginalSkeletonTreeResult<bool> {
        Ok(false)
    }
}

impl OriginalSkeletonContractsTrieConfig {
    pub(crate) fn new() -> Self {
        Self
    }
}
