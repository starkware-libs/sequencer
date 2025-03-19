use ethnum::U256;

use super::utils::Preimage;

#[derive(Debug, thiserror::Error)]
pub enum PatriciaError {
    #[error("Expected a binary node, found: {0:?}")]
    ExpectedBinary(Preimage),
    #[error("Exceeded the max index: {0:?}")]
    MaxLayerIndexExceeded(U256),
}
