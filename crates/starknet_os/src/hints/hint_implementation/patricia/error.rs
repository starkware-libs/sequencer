use num_bigint::BigUint;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::errors::{
    EdgePathError,
    PathToBottomError,
};

use crate::hints::hint_implementation::patricia::utils::{CanonicNode, Preimage};

#[derive(Debug, thiserror::Error)]
pub enum PatriciaError {
    #[error(transparent)]
    EdgePath(#[from] EdgePathError),
    #[error("Expected a binary node, found: {0:?}")]
    ExpectedBinary(Preimage),
    #[error("Expected a branch, found: {0:?}")]
    ExpectedBranch(String),
    #[error("Expected an edge node, found: {0:?}")]
    ExpectedEdge(CanonicNode),
    #[error("Exceeded the max index: {0:?}")]
    MaxLayerIndexExceeded(BigUint),
    #[error("No preimage found for value {0:?}.")]
    MissingPreimage(HashOutput),
    #[error(transparent)]
    PathToBottom(#[from] PathToBottomError),
}
