use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::{GlobalRoot, StateDiffCommitment};
use starknet_committer::block_committer::input::StateDiff;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommitBlockRequest {
    state_diff: StateDiff,
    state_diff_commitment: StateDiffCommitment,
    height: BlockNumber,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CommitBlockResponse {
    state_root: GlobalRoot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RevertBlockRequest {
    reversed_state_diff: StateDiff,
    height: BlockNumber,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RevertBlockResponse {
    state_root: Option<GlobalRoot>,
}
