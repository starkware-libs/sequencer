use crate::patricia_merkle_tree::node_data::leaf::Leaf;

/// Configures the creation of an original skeleton tree.
pub trait OriginalSkeletonTreeConfig<L: Leaf> {
    /// Configures whether modified leaves should be compared to the previous leaves and log out a
    /// warning when encountering a trivial modification.
    fn compare_modified_leaves(&self) -> bool;
}

// TODO(Aviv 05/08/2024): Move this macro to starknet_committer crate
#[macro_export]
macro_rules! generate_trie_config {
    ($struct_name:ident, $leaf_type:ty) => {
        pub struct $struct_name {
            compare_modified_leaves: bool,
        }

        impl $struct_name {
            #[allow(dead_code)]
            pub fn new(compare_modified_leaves: bool) -> Self {
                Self { compare_modified_leaves }
            }
        }

        impl OriginalSkeletonTreeConfig<$leaf_type> for $struct_name {
            fn compare_modified_leaves(&self) -> bool {
                self.compare_modified_leaves
            }
        }
    };
}

#[derive(Default)]
/// Generic config that doesn't compare the modified leaves.
pub struct NoCompareOriginalSkeletonTrieConfig<L: Leaf>(std::marker::PhantomData<L>);

impl<L: Leaf> OriginalSkeletonTreeConfig<L> for NoCompareOriginalSkeletonTrieConfig<L> {
    fn compare_modified_leaves(&self) -> bool {
        false
    }
}
