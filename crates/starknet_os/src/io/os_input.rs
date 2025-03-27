use std::collections::HashMap;

use blockifier::context::ChainInfo;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use shared_execution_objects::central_objects::CentralTransactionExecutionInfo;
use starknet_api::block::{BlockHash, BlockInfo, BlockNumber};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::deprecated_contract_class::ContractClass;
use starknet_api::executable_transaction::Transaction;
use starknet_api::state::StorageKey;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::types::SubTreeHeight;
use starknet_types_core::felt::Felt;

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
// TODO(Nimrod): Remove the `Clone` derive when possible.
#[derive(Debug, Clone)]
pub struct CommitmentInfo {
    pub(crate) previous_root: HashOutput,
    pub(crate) updated_root: HashOutput,
    pub(crate) tree_height: SubTreeHeight,
    // TODO(Dori, 1/8/2025): The value type here should probably be more specific (NodeData<L> for
    //   L: Leaf). This poses a problem in deserialization, as a serialized edge node and a
    //   serialized contract state leaf are both currently vectors of 3 field elements; as the
    //   semantics of the values are unimportant for the OS commitments, we make do with a vector
    //   of field elements as values for now.
    pub(crate) commitment_facts: HashMap<HashOutput, Vec<Felt>>,
}

#[cfg(any(feature = "testing", test))]
impl Default for CommitmentInfo {
    fn default() -> CommitmentInfo {
        CommitmentInfo {
            previous_root: HashOutput::default(),
            updated_root: HashOutput::default(),
            tree_height: SubTreeHeight::ACTUAL_HEIGHT,
            commitment_facts: HashMap::default(),
        }
    }
}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
#[derive(Clone, Debug)]
pub struct ContractClassComponentHashes {
    _contract_class_version: Felt,
    _external_functions_hash: HashOutput,
    _l1_handlers_hash: HashOutput,
    _constructors_hash: HashOutput,
    _abi_hash: HashOutput,
    _sierra_program_hash: HashOutput,
}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
#[cfg_attr(any(test, feature = "testing"), derive(Default))]
#[derive(Debug)]
pub struct OsHints {
    pub os_input: StarknetOsInput,
    pub os_hints_config: OsHintsConfig,
}

/// All input needed to initialize the execution helper.
// TODO(Dori): Add all fields needed to compute commitments, initialize a CachedState and other data
//   required by the execution helper.
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
#[cfg_attr(any(test, feature = "testing"), derive(Default))]
#[derive(Debug)]
pub struct StarknetOsInput {
    pub(crate) contract_state_commitment_info: CommitmentInfo,
    pub(crate) address_to_storage_commitment_info: HashMap<ContractAddress, CommitmentInfo>,
    pub(crate) contract_class_commitment_info: CommitmentInfo,
    pub(crate) chain_info: ChainInfo,
    pub(crate) deprecated_compiled_classes: HashMap<ClassHash, ContractClass>,
    #[allow(dead_code)]
    pub(crate) compiled_classes: HashMap<ClassHash, CasmContractClass>,
    // Note: The Declare tx in the starknet_api crate has a class_info field with a contract_class
    // field. This field is needed by the blockifier, but not used in the OS, so it is expected
    // (and verified) to be initialized with an illegal value, to avoid using it accidentally.
    pub transactions: Vec<Transaction>,
    pub _tx_execution_infos: Vec<CentralTransactionExecutionInfo>,
    // A mapping from Cairo 1 declared class hashes to the hashes of the contract class components.
    pub(crate) declared_class_hash_to_component_hashes:
        HashMap<ClassHash, ContractClassComponentHashes>,
    pub block_info: BlockInfo,
    pub(crate) prev_block_hash: BlockHash,
    pub(crate) new_block_hash: BlockHash,
    // The block number and block hash of the (current_block_number - buffer) block, where
    // buffer=STORED_BLOCK_HASH_BUFFER.
    // It is the hash that is going to be written by this OS run.
    pub(crate) old_block_number_and_hash: Option<(BlockNumber, BlockHash)>,
}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
#[cfg_attr(any(test, feature = "testing"), derive(Default))]
#[derive(Debug)]
pub struct OsHintsConfig {
    pub debug_mode: bool,
    pub full_output: bool,
}

#[derive(Default, Debug)]
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
pub struct CachedStateInput {
    pub(crate) storage: HashMap<ContractAddress, HashMap<StorageKey, Felt>>,
    pub(crate) address_to_class_hash: HashMap<ContractAddress, ClassHash>,
    pub(crate) address_to_nonce: HashMap<ContractAddress, Nonce>,
    pub(crate) class_hash_to_compiled_class_hash: HashMap<ClassHash, CompiledClassHash>,
}
