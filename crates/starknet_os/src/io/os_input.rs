use std::collections::HashMap;

use blockifier::context::ChainInfo;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::deprecated_contract_class::ContractClass;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::types::SubTreeHeight;
use starknet_types_core::felt::Felt;

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
pub struct CommitmentInfo {
    _previous_root: HashOutput,
    _updated_root: HashOutput,
    _tree_height: SubTreeHeight,
    // TODO(Dori, 1/8/2025): The value type here should probably be more specific (NodeData<L> for
    //   L: Leaf). This poses a problem in deserialization, as a serialized edge node and a
    //   serialized contract state leaf are both currently vectors of 3 field elements; as the
    //   semantics of the values are unimportant for the OS commitments, we make do with a vector
    //   of field elements as values for now.
    _commitment_facts: HashMap<HashOutput, Vec<Felt>>,
}

/// All input needed to initialize the execution helper.
// TODO(Dori): Add all fields needed to compute commitments, initialize a CachedState and other data
//   required by the execution helper.
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
pub struct StarknetOsInput {
    _contract_state_commitment_info: CommitmentInfo,
    _address_to_storage_commitment_info: HashMap<ContractAddress, CommitmentInfo>,
    _contract_class_commitment_info: CommitmentInfo,
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
