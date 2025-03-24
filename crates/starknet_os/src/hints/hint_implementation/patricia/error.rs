#[derive(Debug, thiserror::Error)]
pub enum PatriciaError {
    #[error("Expected a binary node")]
    ExpectedBinary,
    #[error("Did not expect an empty node")]
    IsEmpty,
    #[error("Did not expect a leaf node")]
    IsLeaf,
}
