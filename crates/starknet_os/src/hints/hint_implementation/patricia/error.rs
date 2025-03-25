use num_bigint::BigUint;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::errors::{
    EdgePathError,
    PathToBottomError,
};
use starknet_types_core::felt::Felt;

use crate::hints::hint_implementation::patricia::utils::Preimage;

#[derive(Debug, thiserror::Error)]
pub enum PatriciaError {
    #[error(transparent)]
    EdgePath(#[from] EdgePathError),
    #[error("Expected a binary node, found: {0:?}")]
    ExpectedBinary(Preimage),
    #[error("Invalid raw preimage: {0:?}, length should be 2 or 3.")]
    InvalidRawPreimage(Vec<Felt>),
    #[error("Exceeded the max index: {0:?}")]
    MaxLayerIndexExceeded(BigUint),
    #[error("No preimage found for value {0:?}.")]
    MissingPreimage(HashOutput),
    #[error(transparent)]
    PathToBottom(#[from] PathToBottomError),
}
