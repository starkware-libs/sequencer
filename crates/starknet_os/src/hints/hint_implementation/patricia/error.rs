use num_bigint::BigUint;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::errors::{
    EdgePathError,
    PathToBottomError,
    PreimageError,
};

#[derive(Debug, thiserror::Error)]
pub enum PatriciaError {
    #[error(transparent)]
    EdgePath(#[from] EdgePathError),
    #[error("Exceeded the max index: {0:?}")]
    MaxLayerIndexExceeded(BigUint),
    #[error("No preimage found for value {0:?}.")]
    MissingPreimage(HashOutput),
    #[error(transparent)]
    PathToBottom(#[from] PathToBottomError),
    #[error(transparent)]
    Preimage(#[from] PreimageError),
}
