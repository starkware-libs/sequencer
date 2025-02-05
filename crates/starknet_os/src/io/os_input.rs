use std::collections::HashMap;

use blockifier::context::ChainInfo;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::ContractAddress;
use starknet_api::deprecated_contract_class::ContractClass;
use starknet_committer::block_committer::input::StarknetStorageValue;
use starknet_committer::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use starknet_committer::patricia_merkle_tree::types::{ClassHash, CompiledClassHash};
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
    _storage_commitments: HashMap<ContractAddress, CommitmentInfo<StarknetStorageValue>>,
    _class_commitments: CommitmentInfo<CompiledClassHash>,
    _deprecated_compiled_classes: HashMap<ClassHash, ContractClass>,
    _compiled_classes: HashMap<ClassHash, CasmContractClass>,
    _chain_info: ChainInfo,
    _prev_block_hash: BlockHash,
    _new_block_hash: BlockHash,
    // The block number and block hash of the (current_block_number - buffer) block, where
    // buffer=STORED_BLOCK_HASH_BUFFER.
    // It is the hash that is going to be written by this OS run.
    _old_block_number_and_hash: Option<(BlockNumber, BlockHash)>,
    _debug_mode: bool,
    _full_output: bool,
}
