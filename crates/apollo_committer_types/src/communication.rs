use std::sync::Arc;

use async_trait::async_trait;
use starknet_committer::block_committer::input::StateDiff;
use starknet_committer::hash_function::hash::StateRoots;

use crate::errors::CommitterClientResult;

#[async_trait]
#[cfg_attr(any(feature = "testing", test), mockall::automock)]
pub trait CommitterClient: Send + Sync {
    /// Applies the state diff on the state trees and computes the new state roots.
    async fn commit_block(
        &self,
        state_diff: StateDiff,
        prev_state_roots: StateRoots,
    ) -> CommitterClientResult<StateRoots>;
}

pub type SharedCommitterClient = Arc<dyn CommitterClient>;
