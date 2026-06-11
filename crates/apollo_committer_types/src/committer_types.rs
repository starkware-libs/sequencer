#[cfg(feature = "os_input")]
pub use blockifier::state::accessed_keys::AccessedKeys;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::{GlobalRoot, StateDiffCommitment};
use starknet_api::state::ThinStateDiff;
#[cfg(feature = "os_input")]
use starknet_committer::patricia_merkle_tree::types::StateCommitmentInfos;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommitBlockRequest {
    pub state_diff: ThinStateDiff,
    // Field is optional because for old blocks, the state diff commitment might not be available.
    pub state_diff_commitment: Option<StateDiffCommitment>,
    pub height: BlockNumber,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommitBlockResponse {
    pub global_root: GlobalRoot,
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

/// Commit a block and return merged Patricia witness proofs for OS input (pre- and post-commit
/// paths).
#[cfg(feature = "os_input")]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReadPathsAndCommitBlockRequest {
    pub commit: CommitBlockRequest,
    pub accessed_keys: AccessedKeys,
}

#[cfg(feature = "os_input")]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReadPathsAndCommitBlockResponse {
    pub global_root: GlobalRoot,
    pub state_commitment_infos: StateCommitmentInfos,
}
