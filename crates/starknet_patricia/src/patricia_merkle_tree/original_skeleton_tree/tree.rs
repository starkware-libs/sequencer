use std::collections::HashMap;
use std::future::Future;

use starknet_api::hash::HashOutput;
use starknet_patricia_storage::storage_trait::Storage;

use crate::patricia_merkle_tree::node_data::leaf::{Leaf, LeafModifications};
use crate::patricia_merkle_tree::original_skeleton_tree::config::{
    NoCompareOriginalSkeletonTrieConfig,
    OriginalSkeletonTreeConfig,
};
use crate::patricia_merkle_tree::original_skeleton_tree::errors::OriginalSkeletonTreeError;
use crate::patricia_merkle_tree::original_skeleton_tree::node::OriginalSkeletonNode;
use crate::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};

pub type OriginalSkeletonNodeMap = HashMap<NodeIndex, OriginalSkeletonNode>;
pub type OriginalSkeletonTreeResult<T> = Result<T, OriginalSkeletonTreeError>;

/// Consider a Patricia-Merkle Tree which should be updated with new leaves.
/// This trait represents the structure of the subtree which will be modified in the
/// update. It also contains the hashes (for edge siblings - also the edge data) of the unmodified
/// nodes on the Merkle paths from the updated leaves to the root.
pub trait OriginalSkeletonTree<'a>: Sized {
    // Use explicit desugaring of `async fn` to allow adding trait bounds to the return type, see
    // https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits.html#async-fn-in-public-traits
    // for details.
    fn create<L: Leaf>(
        storage: &mut (impl Storage + Send + Sync),
        root_hash: HashOutput,
        sorted_leaf_indices: SortedLeafIndices<'a>,
        config: &(impl OriginalSkeletonTreeConfig<L> + Sync),
        leaf_modifications: &LeafModifications<L>,
    ) -> impl Future<Output = OriginalSkeletonTreeResult<Self>> + Send;

    fn get_nodes(&self) -> &OriginalSkeletonNodeMap;

    fn get_nodes_mut(&mut self) -> &mut OriginalSkeletonNodeMap;

    // Use explicit desugaring of `async fn` to allow adding trait bounds to the return type, see
    // https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits.html#async-fn-in-public-traits
    // for details.
    fn create_and_get_previous_leaves<L: Leaf>(
        storage: &mut (impl Storage + Send + Sync),
        root_hash: HashOutput,
        sorted_leaf_indices: SortedLeafIndices<'a>,
        config: &(impl OriginalSkeletonTreeConfig<L> + Sync),
        leaf_modifications: &LeafModifications<L>,
    ) -> impl Future<Output = OriginalSkeletonTreeResult<(Self, HashMap<NodeIndex, L>)>> + Send;

    #[allow(dead_code)]
    fn get_sorted_leaf_indices(&self) -> SortedLeafIndices<'a>;
}

// TODO(Dori, 1/7/2024): Make this a tuple struct.
#[derive(Debug, PartialEq)]
pub struct OriginalSkeletonTreeImpl<'a> {
    pub nodes: HashMap<NodeIndex, OriginalSkeletonNode>,
    pub sorted_leaf_indices: SortedLeafIndices<'a>,
}

impl<'a> OriginalSkeletonTree<'a> for OriginalSkeletonTreeImpl<'a> {
    async fn create<L: Leaf>(
        storage: &mut (impl Storage + Send + Sync),
        root_hash: HashOutput,
        sorted_leaf_indices: SortedLeafIndices<'a>,
        config: &(impl OriginalSkeletonTreeConfig<L> + Sync),
        leaf_modifications: &LeafModifications<L>,
    ) -> OriginalSkeletonTreeResult<Self> {
        Self::create_impl(storage, root_hash, sorted_leaf_indices, config, leaf_modifications).await
    }

    fn get_nodes(&self) -> &OriginalSkeletonNodeMap {
        &self.nodes
    }

    fn get_nodes_mut(&mut self) -> &mut OriginalSkeletonNodeMap {
        &mut self.nodes
    }

    async fn create_and_get_previous_leaves<L: Leaf>(
        storage: &mut (impl Storage + Send + Sync),
        root_hash: HashOutput,
        sorted_leaf_indices: SortedLeafIndices<'a>,
        config: &(impl OriginalSkeletonTreeConfig<L> + Sync),
        leaf_modifications: &LeafModifications<L>,
    ) -> OriginalSkeletonTreeResult<(Self, HashMap<NodeIndex, L>)> {
        Self::create_and_get_previous_leaves_impl(
            storage,
            root_hash,
            sorted_leaf_indices,
            leaf_modifications,
            config,
        )
        .await
    }

    fn get_sorted_leaf_indices(&self) -> SortedLeafIndices<'a> {
        self.sorted_leaf_indices
    }
}

impl<'a> OriginalSkeletonTreeImpl<'a> {
    pub async fn get_leaves<L: Leaf>(
        storage: &mut (impl Storage + Send + Sync),
        root_hash: HashOutput,
        sorted_leaf_indices: SortedLeafIndices<'a>,
    ) -> OriginalSkeletonTreeResult<HashMap<NodeIndex, L>> {
        let config = NoCompareOriginalSkeletonTrieConfig::default();
        let leaf_modifications = LeafModifications::new();
        let (_, previous_leaves) = Self::create_and_get_previous_leaves_impl(
            storage,
            root_hash,
            sorted_leaf_indices,
            &leaf_modifications,
            &config,
        )
        .await?;
        Ok(previous_leaves)
    }
}
