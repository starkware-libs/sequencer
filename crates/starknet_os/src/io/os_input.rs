use std::collections::HashMap;

use blockifier::context::ChainInfo;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use shared_execution_objects::objects::CentralTransactionExecutionInfo;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::deprecated_contract_class::ContractClass;
use starknet_api::executable_transaction::Transaction;
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

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
pub struct ContractClassComponentHashes {
    _contract_class_version: Felt,
    _external_functions_hash: HashOutput,
    _l1_handlers_hash: HashOutput,
    _constructors_hash: HashOutput,
    _abi_hash: HashOutput,
    _sierra_program_hash: HashOutput,
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
    _transactions: Vec<Transaction>,
    _tx_execution_infos: Vec<CentralTransactionExecutionInfo>,
    // A mapping from Cairo 1 declared class hashes to the hashes of the contract class components.
    _declared_class_hash_to_component_hashes: HashMap<ClassHash, ContractClassComponentHashes>,
    _prev_block_hash: BlockHash,
    _new_block_hash: BlockHash,
    // The block number and block hash of the (current_block_number - buffer) block, where
    // buffer=STORED_BLOCK_HASH_BUFFER.
    // It is the hash that is going to be written by this OS run.
    _old_block_number_and_hash: Option<(BlockNumber, BlockHash)>,
    _debug_mode: bool,
    _full_output: bool,
}
