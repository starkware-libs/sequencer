use std::borrow::Borrow;
use std::collections::HashMap;
use std::fmt::Debug;

use starknet_patricia::patricia_merkle_tree::node_data::leaf::{Leaf, LeafModifications};
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::config::OriginalSkeletonTreeConfig;
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::tree::{
    OriginalSkeletonTreeImpl,
    OriginalSkeletonTreeResult,
};
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use tracing::warn;

/// Logs out a warning of a trivial modification.
macro_rules! log_trivial_modification {
    ($index:expr, $value:expr) => {
        warn!("Encountered a trivial modification at index {:?}, with value {:?}", $index, $value);
    };
}

pub(crate) use log_trivial_modification;

/// Given leaf indices that were previously empty leaves, logs out a warning for trivial
/// modification if a leaf is modified to an empty leaf.
/// If this check is suppressed by configuration, does nothing.
pub(crate) fn log_warning_for_empty_leaves<L: Leaf, T: Borrow<NodeIndex> + Debug>(
    leaf_indices: &[T],
    leaf_modifications: &LeafModifications<L>,
    config: &impl OriginalSkeletonTreeConfig<L>,
) -> OriginalSkeletonTreeResult<()> {
    if !config.compare_modified_leaves() {
        return Ok(());
    }
    let empty_leaf = L::default();
    for leaf_index in leaf_indices {
        if L::compare(leaf_modifications, leaf_index.borrow(), &empty_leaf)? {
            log_trivial_modification!(leaf_index, empty_leaf);
        }
    }
    Ok(())
}

pub(crate) fn handle_empty_subtree<'a, L: Leaf>(
    sorted_leaf_indices: SortedLeafIndices<'a>,
) -> (OriginalSkeletonTreeImpl<'a>, HashMap<NodeIndex, L>) {
    (
        OriginalSkeletonTreeImpl::create_empty(sorted_leaf_indices),
        sorted_leaf_indices.get_indices().iter().map(|idx| (*idx, L::default())).collect(),
    )
}
