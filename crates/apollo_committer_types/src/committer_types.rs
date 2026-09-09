pub use blockifier::state::accessed_keys::AccessedKeys;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::{GlobalRoot, StateDiffCommitment};
use starknet_api::state::ThinStateDiff;
use starknet_committer::patricia_merkle_tree::types::CompressedStateCommitmentInfos;

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
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReadPathsAndCommitBlockRequest {
    pub commit: CommitBlockRequest,
    pub accessed_keys: AccessedKeys,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReadPathsAndCommitBlockResponse {
    pub global_root: GlobalRoot,
    pub state_commitment_infos: CompressedStateCommitmentInfos,
}

/// Read the stored state commitment infos of the committed heights in `[start_height,
/// end_height)`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetStateCommitmentInfosRequest {
    pub start_height: BlockNumber,
    /// Exclusive.
    pub end_height: BlockNumber,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateCommitmentInfosAtHeight {
    pub height: BlockNumber,
    pub state_commitment_infos: CompressedStateCommitmentInfos,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GetStateCommitmentInfosResponse {
    /// Ascending by height; heights without stored infos are absent.
    pub state_commitment_infos: Vec<StateCommitmentInfosAtHeight>,
}
