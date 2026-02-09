use apollo_storage::StorageError;
use starknet_api::StarknetApiError;
use starknet_api::block::BlockNumber;
use starknet_api::core::StateDiffCommitment;

#[derive(Debug, thiserror::Error)]
pub enum CommitmentManagerError {
    #[error("Block hash for block: {0} not found in storage.")]
    MissingBlockHash(BlockNumber),
    #[error("Partial block hash components for block: {0} not found in storage.")]
    MissingPartialBlockHashComponents(BlockNumber),
    #[error(transparent)]
    StarknetApi(#[from] StarknetApiError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(
        "Wrong commitment task height. Expected: {expected}, Actual: {actual}. State diff \
         commitment: {state_diff_commitment:?}"
    )]
    WrongCommitmentTaskHeight {
        expected: BlockNumber,
        actual: BlockNumber,
        state_diff_commitment: Option<StateDiffCommitment>,
    },
    #[error("Wrong revert task height. Expected: {expected}, Actual: {actual}.")]
    WrongRevertTaskHeight { expected: BlockNumber, actual: BlockNumber },
}
