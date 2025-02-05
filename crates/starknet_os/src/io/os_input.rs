use std::collections::HashMap;

use starknet_committer::block_committer::input::StarknetStorageValue;
use starknet_committer::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use starknet_committer::patricia_merkle_tree::types::CompiledClassHash;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::NodeData;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia::patricia_merkle_tree::types::SubTreeHeight;

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
pub struct CommitmentInfo<L: Leaf> {
    _previous_root: HashOutput,
    _updated_root: HashOutput,
    _tree_height: SubTreeHeight,
    _commitment_facts: HashMap<HashOutput, NodeData<L>>,
}

/// All input needed to initialize the execution helper.
// TODO(Dori): Add all fields needed to compute commitments, initialize a CachedState and other data
//   required by the execution helper.
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
pub struct StarknetOsInput {
    _contract_commitments: CommitmentInfo<ContractState>,
    _storage_commitments: CommitmentInfo<StarknetStorageValue>,
    _class_commitments: CommitmentInfo<CompiledClassHash>,
}
