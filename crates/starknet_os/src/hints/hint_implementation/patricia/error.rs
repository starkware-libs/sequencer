use ethnum::U256;
use starknet_patricia::patricia_merkle_tree::types::SubTreeHeight;

use super::utils::{LayerIndex, Preimage};

#[derive(Debug, thiserror::Error)]
pub enum PatriciaError {
    #[error(
        "Children of node at height {height} with LayerIndex {index} are None. Node should be \
         None."
    )]
    BothChildrenAreNone { index: LayerIndex, height: SubTreeHeight },
    #[error("Expected a binary node, found: {0:?}")]
    ExpectedBinary(Preimage),
    #[error("Expected that one of the children is not None, found both None.")]
    InvalidInnerNode,
    #[error("Exceeded the max index: {0:?}")]
    MaxLayerIndexExceeded(U256),
}
