use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::{GlobalRoot, StateDiffCommitment};
use starknet_api::state::ThinStateDiff;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommitBlockRequest {
    state_diff: ThinStateDiff,
    state_diff_commitment: StateDiffCommitment,
    height: BlockNumber,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CommitBlockResponse {
    state_root: GlobalRoot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RevertBlockRequest {
    // A synthetic state diff that undoes the state diff of the given height.
    reversed_state_diff: ThinStateDiff,
    height: BlockNumber,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RevertBlockResponse {
    Uncommitted,
    RevertedTo(GlobalRoot),
}
