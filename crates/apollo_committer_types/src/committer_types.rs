use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::{GlobalRoot, StateDiffCommitment};
use starknet_api::state::ThinStateDiff;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommitBlockRequest {
    pub state_diff: ThinStateDiff,
    // Field is optional because for old blocks, the state diff commitment might not be available.
    pub state_diff_commitment: Option<StateDiffCommitment>,
    pub height: BlockNumber,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommitBlockResponse {
    pub state_root: GlobalRoot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RevertBlockRequest {
    // A synthetic state diff that undoes the state diff of the given height.
    pub reversed_state_diff: ThinStateDiff,
    pub height: BlockNumber,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RevertBlockResponse {
    // Nothing to revert, the committer had the resulted state root.
    AlreadyReverted(GlobalRoot),
    // The block was reverted, return the state root after reverting the state.
    RevertedTo(GlobalRoot),
    // Nothing to revert. A future block that has not been committed.
    Uncommitted,
}
