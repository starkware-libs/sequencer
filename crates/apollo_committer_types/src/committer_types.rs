use serde::{Deserialize, Serialize};
use starknet_api::core::GlobalRoot;
use starknet_api::state::ThinStateDiff;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::{Poseidon, StarkHash};

/// Roots of the patricia tries that represent the state.
#[derive(Debug, Default, Copy, Clone, Serialize, Deserialize)]
pub struct StateCommitment {
    pub contracts_trie_root: HashOutput,
    pub classes_trie_root: HashOutput,
}

/// Input for committing state changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateCommitmentInput {
    pub last_state: StateCommitment,
    pub state_diff: ThinStateDiff,
}

impl StateCommitment {
    pub fn to_global_root(self) -> GlobalRoot {
        if self.contracts_trie_root == HashOutput::ROOT_OF_EMPTY_TREE
            && self.classes_trie_root == HashOutput::ROOT_OF_EMPTY_TREE
        {
            GlobalRoot(Felt::ZERO)
        } else {
            let hash_input =
                [GlobalRoot::STATE_VERSION, self.contracts_trie_root.0, self.classes_trie_root.0];
            GlobalRoot(Poseidon::hash_array(&hash_input))
        }
    }
}
