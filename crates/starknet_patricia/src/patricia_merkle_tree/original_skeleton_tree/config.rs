use crate::patricia_merkle_tree::node_data::leaf::{Leaf, LeafModifications};
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTreeResult;
use crate::patricia_merkle_tree::types::NodeIndex;

/// Configures the creation of an original skeleton tree.
pub trait OriginalSkeletonTreeConfig<L: Leaf> {
    /// Configures whether modified leaves should be compared to the previous leaves and log out a
    /// warning when encountering a trivial modification.
    fn compare_modified_leaves(&self) -> bool;

    /// Compares the previous leaf to the modified and returns true if they are equal.
    fn compare_leaf(
        &self,
        leaf_modifications: &LeafModifications<L>,
        index: &NodeIndex,
        previous_leaf: &L,
    ) -> OriginalSkeletonTreeResult<bool>;
}

// TODO(Aviv 05/08/2024): Move this macro to starknet_committer crate
#[macro_export]
macro_rules! generate_trie_config {
    ($struct_name:ident, $leaf_type:ty) => {
        pub struct $struct_name{
            compare_modified_leaves: bool,
        }

        impl $struct_name{
            #[allow(dead_code)]
            pub fn new(
                compare_modified_leaves: bool,
            ) -> Self {
                Self {compare_modified_leaves }
            }
        }

        impl OriginalSkeletonTreeConfig<$leaf_type> for $struct_name{
            fn compare_modified_leaves(&self) -> bool {
                self.compare_modified_leaves
            }

            fn compare_leaf(
                &self,
                leaf_modifications: &$crate::patricia_merkle_tree::node_data::leaf::LeafModifications<$leaf_type>,
                index: &NodeIndex,
                previous_leaf: &$leaf_type,
            ) -> OriginalSkeletonTreeResult<bool> {
                let new_leaf =
                    leaf_modifications
                    .get(index)
                    .ok_or(OriginalSkeletonTreeError::ReadModificationsError(*index))?;
                Ok(new_leaf == previous_leaf)
            }
        }
    };
}
