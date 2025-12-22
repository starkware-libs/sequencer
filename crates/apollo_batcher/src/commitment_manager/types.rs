#![allow(dead_code)]
use apollo_storage::StorageError;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::{GlobalRoot, StateDiffCommitment};
use starknet_api::state::ThinStateDiff;
use starknet_api::StarknetApiError;

/// Input for commitment tasks.
pub(crate) struct CommitmentTaskInput {
    pub(crate) state_diff: ThinStateDiff,
    pub(crate) height: BlockNumber,
    // Field is optional because for old blocks, the state diff commitment might not be available.
    pub(crate) state_diff_commitment: Option<StateDiffCommitment>,
}

/// Output of commitment tasks.
pub(crate) struct CommitmentTaskOutput {
    pub(crate) global_root: GlobalRoot,
    pub(crate) height: BlockNumber,
}

pub(crate) struct FinalBlockCommitment {
    pub(crate) height: BlockNumber,
    pub(crate) block_hash: BlockHash,
    pub(crate) global_root: GlobalRoot,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum CommitmentManagerError {
    #[error(transparent)]
    StarknetApi(#[from] StarknetApiError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("Block hash for block: {0} not found in storage.")]
    MissingBlockHash(BlockNumber),
    #[error("Partial block hash components for block: {0} not found in storage.")]
    MissingPartialBlockHashComponents(BlockNumber),
}

pub(crate) type CommitmentManagerResult<T> = Result<T, CommitmentManagerError>;
