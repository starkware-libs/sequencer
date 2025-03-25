#[derive(Debug, thiserror::Error)]
pub enum PatriciaError {
    #[error("Expected a binary node")]
    ExpectedBinary,
}
