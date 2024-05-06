use crate::block_committer::errors::BlockCommitmentError;

#[allow(dead_code)]
type BlockCommitmentResult<T> = Result<T, BlockCommitmentError>;
