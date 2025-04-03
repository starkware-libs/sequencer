use super::utils::Preimage;

#[derive(Debug, thiserror::Error)]
pub enum PatriciaError {
    #[error("Expected a binary node, found: {0:?}")]
    ExpectedBinary(Preimage),
}
