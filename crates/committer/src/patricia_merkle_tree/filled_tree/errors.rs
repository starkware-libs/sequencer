use tokio::task::JoinError;

use crate::block_committer::input::StarknetStorageValue;
use crate::patricia_merkle_tree::filled_tree::node::{CompiledClassHash, FilledNode};
use crate::patricia_merkle_tree::node_data::errors::LeafError;
use crate::patricia_merkle_tree::node_data::leaf::{ContractState, Leaf};
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::patricia_merkle_tree::updated_skeleton_tree::errors::UpdatedSkeletonTreeError;

#[derive(thiserror::Error, Debug)]
pub enum FilledTreeError<L: Leaf> {
    #[error("Deleted leaf at index {0:?} appears in the updated skeleton tree.")]
    DeletedLeafInSkeleton(NodeIndex),
    #[error("Double update at node {index:?}. Existing value: {existing_value:?}.")]
    DoubleOutputUpdate { index: NodeIndex, existing_value: Box<FilledNode<L>> },
    #[error("Double update for leaf {index:?}. Existing value: {existing_value:?}.")]
    DoubleLeafOutputUpdate { index: NodeIndex, existing_value: L::O },
    #[error(transparent)]
    Leaf(#[from] LeafError),
    #[error("Missing node at index {0:?}.")]
    MissingNode(NodeIndex),
    #[error("Missing root.")]
    MissingRoot,
    #[error("Poisoned lock: {0}.")]
    PoisonedLock(String),
    #[error(transparent)]
    SerializeError(#[from] serde_json::Error),
    #[error(transparent)]
    UpdatedSkeletonError(#[from] UpdatedSkeletonTreeError),
    #[error(transparent)]
    JoinError(#[from] JoinError),
}

pub type StorageTrieError = FilledTreeError<StarknetStorageValue>;
pub type ClassesTrieError = FilledTreeError<CompiledClassHash>;
pub type ContractsTrieError = FilledTreeError<ContractState>;
