use serde::{Deserialize, Serialize};
use starknet_committer::block_committer::input::StateDiff;
use starknet_committer::hash_function::hash::StateRoots;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommitBlockRequest {
    state_diff: StateDiff,
    prev_state_roots: StateRoots,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CommitBlockResponse {
    new_state_roots: StateRoots,
}
