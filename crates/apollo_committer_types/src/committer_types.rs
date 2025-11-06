use serde::{Deserialize, Serialize};
use starknet_committer::block_committer::input::StateDiff;
use starknet_committer::hash_function::hash::StateRoots;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommitBlockInput {
    pub state_diff: StateDiff,
    pub prev_state_roots: StateRoots,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommitBlockResponse {
    pub new_state_roots: StateRoots,
}
