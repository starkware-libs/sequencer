use crate::patricia_merkle_tree::node_data::leaf::LeafData;
use crate::patricia_merkle_tree::types::NodeIndex;

use crate::patricia_merkle_tree::filled_tree::node::FilledNode;

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum UpdatedSkeletonTreeError<L: LeafData> {
    MissingNode(NodeIndex),
    DoubleUpdate {
        index: NodeIndex,
        existing_value: Box<FilledNode<L>>,
    },
    PoisonedLock(String),
    NonDroppedPointer(String),
}
