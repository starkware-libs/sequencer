#![allow(dead_code)]
use starknet_api::block::BlockNumber;
use starknet_api::state::ThinStateDiff;

/// Input for commitment tasks.
pub struct CommitmentTaskInput {
    pub(crate) state_diff: ThinStateDiff,
    pub(crate) height: BlockNumber,
}
