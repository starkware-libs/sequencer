use starknet_api::block::BlockNumber;
use starknet_api::core::StateDiffCommitment;

#[derive(Debug, thiserror::Error)]
pub enum CommitmentManagerError {
    #[error(
        "Wrong commitment task height. Expected: {expected}, Actual: {actual}. State diff \
         commitment: {state_diff_commitment:?}"
    )]
    WrongTaskHeight {
        expected: BlockNumber,
        actual: BlockNumber,
        state_diff_commitment: Option<StateDiffCommitment>,
    },
}
