use crate::patricia_merkle_tree::types::NodeIndex;

#[derive(Debug, thiserror::Error)]
pub enum UpdatedSkeletonTreeError {
    #[error("Missing node at index {0:?}.")]
    MissingNode(NodeIndex),
}
