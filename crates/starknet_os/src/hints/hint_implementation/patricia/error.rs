use num_bigint::BigUint;

use crate::hints::hint_implementation::patricia::utils::{CanonicNode, Preimage};

#[derive(Debug, thiserror::Error)]
pub enum PatriciaError {
    #[error("Expected a binary node, found: {0:?}")]
    ExpectedBinary(Preimage),
    #[error("Expected an edge node, found: {0:?}")]
    ExpectedEdge(CanonicNode),
    #[error("Exceeded the max index: {0:?}")]
    MaxLayerIndexExceeded(BigUint),
}
