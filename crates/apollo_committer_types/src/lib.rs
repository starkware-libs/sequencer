pub mod errors;

use std::sync::Arc;

use async_trait::async_trait;
use errors::CommitterClientResult;
use serde::{Deserialize, Serialize};
use starknet_committer::block_committer::input::StateDiff;
use starknet_patricia::hash::hash_trait::HashOutput;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StateRoots {
    pub contracts_trie_root_hash: HashOutput,
    pub classes_trie_root_hash: HashOutput,
}

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
