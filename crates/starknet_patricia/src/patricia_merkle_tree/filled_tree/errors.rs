use tokio::task::JoinError;

use crate::patricia_merkle_tree::node_data::errors::LeafError;
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::patricia_merkle_tree::updated_skeleton_tree::errors::UpdatedSkeletonTreeError;

#[derive(thiserror::Error, Debug)]
pub enum FilledTreeError {
    #[error("Deleted leaf at index {0:?} appears in the updated skeleton tree.")]
    DeletedLeafInSkeleton(NodeIndex),
    #[error("Double update at node {index:?}. Existing value: {existing_value_as_string:?}.")]
    DoubleUpdate { index: NodeIndex, existing_value_as_string: String },
    #[error("Got the following error at leaf index {leaf_index:?}: {leaf_error:?}")]
    Leaf { leaf_error: LeafError, leaf_index: NodeIndex },
    #[error("Missing node placeholder at index {0:?}.")]
    MissingNodePlaceholder(NodeIndex),
    #[error("Missing leaf input for index {0:?}.")]
    MissingLeafInput(NodeIndex),
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
