use crate::{block_committer::input::ContractAddress, patricia_merkle_tree::types::NodeIndex};

#[derive(Debug, thiserror::Error)]
pub enum UpdatedSkeletonTreeError {
    #[error("Missing node at index {0:?}.")]
    MissingNode(NodeIndex),
    #[error("Missing input: Couldn't build the skeleton at address {0:?}")]
    LowerTreeCommitmentError(ContractAddress),
}
