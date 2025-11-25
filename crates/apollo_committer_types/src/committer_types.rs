use serde::{Deserialize, Serialize};
use starknet_api::hash::StateRoots;
use starknet_committer::block_committer::input::StateDiff;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommitBlockRequest {
    state_diff: StateDiff,
    prev_state_roots: StateRoots,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CommitBlockResponse {
    new_state_roots: StateRoots,
}
