use serde::{Deserialize, Serialize};
use starknet_api::state::ThinStateDiff;

/// Input for commitments calculation of a block.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommitmentInput {
    pub state_diff: ThinStateDiff,
    // TODO(Nimrod): Add more fields, should be everything needed for block hash calculation except
    // the global root.
}
