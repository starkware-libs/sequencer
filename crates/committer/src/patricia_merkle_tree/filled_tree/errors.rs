use crate::patricia_merkle_tree::filled_tree::node::FilledNode;
use crate::patricia_merkle_tree::node_data::leaf::SkeletonLeaf;
use crate::patricia_merkle_tree::updated_skeleton_tree::errors::UpdatedSkeletonTreeError;
use crate::patricia_merkle_tree::{node_data::leaf::LeafData, types::NodeIndex};

#[derive(thiserror::Error, Debug)]
pub(crate) enum FilledTreeError<L: LeafData> {
    #[error("Double update at node {index:?}. Existing value: {existing_value}.")]
    DoubleUpdate {
        index: NodeIndex,
        existing_value: Box<FilledNode<L>>,
    },
    #[error("Modification of index {index:?} does not match the skeleton leaf {skeleton_leaf:?}.")]
    // TODO(Dori, 1/6/2024): Add existing modification value to the inconsistency error.
    InconsistentModification {
        index: NodeIndex,
        // TODO(Dori, 1/6/2024): Remove the Box<> once the skeleton leaf no longer contains data.
        skeleton_leaf: Box<SkeletonLeaf>,
    },
    #[error("Missing modification data at index {0:?}.")]
    MissingDataForUpdate(NodeIndex),
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
}
