use crate::patricia_merkle_tree::node_data::leaf::LeafData;
use crate::patricia_merkle_tree::types::NodeIndex;

use crate::patricia_merkle_tree::filled_tree::node::FilledNode;

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum UpdatedSkeletonTreeError<L: LeafData> {
    MissingDataForUpdate(NodeIndex),
    MissingNode(NodeIndex),
    DoubleUpdate {
        index: NodeIndex,
        existing_value: Box<FilledNode<L>>,
    },
    // TODO(Dori, 1/6/2024): Add existing node value + modification values to the inconsistency
    //   error.
    InconsistentModification(NodeIndex),
    PoisonedLock(String),
    NonDroppedPointer(String),
}
