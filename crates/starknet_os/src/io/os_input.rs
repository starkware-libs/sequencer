use std::collections::{BTreeMap, HashMap};

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use serde::Serialize;
use shared_execution_objects::central_objects::CentralTransactionExecutionInfo;
use starknet_api::block::{BlockHash, BlockInfo, BlockNumber};
#[cfg(feature = "deserialize")]
use starknet_api::core::deserialize_chain_id_from_hex;
use starknet_api::core::{ChainId, ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::deprecated_contract_class::ContractClass;
use starknet_api::executable_transaction::Transaction;
use starknet_api::state::{ContractClassComponentHashes, StorageKey};
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::types::SubTreeHeight;
use starknet_types_core::felt::Felt;
use tracing::level_filters::LevelFilter;

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
#[cfg_attr(feature = "deserialize", serde(deny_unknown_fields))]
#[derive(Debug)]
pub struct CommitmentInfo {
    pub previous_root: HashOutput,
    pub updated_root: HashOutput,
    pub tree_height: SubTreeHeight,
    // TODO(Dori, 1/8/2025): The value type here should probably be more specific (NodeData<L> for
    //   L: Leaf). This poses a problem in deserialization, as a serialized edge node and a
    //   serialized contract state leaf are both currently vectors of 3 field elements; as the
    //   semantics of the values are unimportant for the OS commitments, we make do with a vector
    //   of field elements as values for now.
    pub commitment_facts: HashMap<HashOutput, Vec<Felt>>,
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
#[cfg_attr(feature = "deserialize", serde(deny_unknown_fields))]
#[cfg_attr(any(test, feature = "testing"), derive(Default))]
#[derive(Debug)]
pub struct OsHints {
    pub os_input: StarknetOsInput,
    pub os_hints_config: OsHintsConfig,
}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
#[cfg_attr(feature = "deserialize", serde(deny_unknown_fields))]
#[cfg_attr(any(test, feature = "testing"), derive(Default))]
#[derive(Debug)]
pub struct StarknetOsInput {
    pub os_block_inputs: Vec<OsBlockInput>,
    pub cached_state_inputs: Vec<CachedStateInput>,
    pub deprecated_compiled_classes: BTreeMap<CompiledClassHash, ContractClass>,
    pub compiled_classes: BTreeMap<CompiledClassHash, CasmContractClass>,
}

// TODO(Meshi): Remove Once the blockifier ChainInfo do not support deprecated fee token.
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
#[cfg_attr(feature = "deserialize", serde(deny_unknown_fields))]
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OsChainInfo {
    #[cfg_attr(feature = "deserialize", serde(deserialize_with = "deserialize_chain_id_from_hex"))]
    pub chain_id: ChainId,
    pub strk_fee_token_address: ContractAddress,
}

impl Default for OsChainInfo {
    fn default() -> Self {
        OsChainInfo {
            chain_id: ChainId::Other("0x0".to_string()),
            strk_fee_token_address: ContractAddress::default(),
        }
    }
}

/// All input needed to initialize the execution helper.
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
#[cfg_attr(feature = "deserialize", serde(deny_unknown_fields))]
#[cfg_attr(any(test, feature = "testing"), derive(Default))]
#[derive(Debug)]
pub struct OsBlockInput {
    pub contract_state_commitment_info: CommitmentInfo,
    pub address_to_storage_commitment_info: HashMap<ContractAddress, CommitmentInfo>,
    pub contract_class_commitment_info: CommitmentInfo,
    // Note: The Declare tx in the starknet_api crate has a class_info field with a contract_class
    // field. This field is needed by the blockifier, but not used in the OS, so it is expected
    // (and verified) to be initialized with an illegal value, to avoid using it accidentally.
    pub transactions: Vec<Transaction>,
    pub tx_execution_infos: Vec<CentralTransactionExecutionInfo>,
    // A mapping from Cairo 1 declared class hashes to the hashes of the contract class components.
    pub declared_class_hash_to_component_hashes: HashMap<ClassHash, ContractClassComponentHashes>,
    pub block_info: BlockInfo,
    pub prev_block_hash: BlockHash,
    pub new_block_hash: BlockHash,
    // The block number and block hash of the (current_block_number - buffer) block, where
    // buffer=STORED_BLOCK_HASH_BUFFER.
    // It is the hash that is going to be written by this OS run.
    pub old_block_number_and_hash: Option<(BlockNumber, BlockHash)>,
    // A map from Class hashes to Compiled class hashes v2 for all classes that require migration.
    #[allow(dead_code)]
    // TODO(AvivG): remove allow(dead_code) once migration_class_hashes is used.
    pub class_hashes_to_migrate: HashMap<ClassHash, CompiledClassHash>,
}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
#[cfg_attr(feature = "deserialize", serde(deny_unknown_fields))]
#[cfg_attr(any(test, feature = "testing"), derive(Default))]
#[derive(Debug)]
pub struct OsHintsConfig {
    pub debug_mode: bool,
    pub full_output: bool,
    pub use_kzg_da: bool,
    pub chain_info: OsChainInfo,
}

impl OsHintsConfig {
    pub fn log_level(&self) -> LevelFilter {
        if self.debug_mode { LevelFilter::DEBUG } else { LevelFilter::INFO }
    }
}

#[derive(Default, Debug)]
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
#[cfg_attr(feature = "deserialize", serde(deny_unknown_fields))]
pub struct CachedStateInput {
    pub storage: HashMap<ContractAddress, HashMap<StorageKey, Felt>>,
    pub address_to_class_hash: HashMap<ContractAddress, ClassHash>,
    pub address_to_nonce: HashMap<ContractAddress, Nonce>,
    pub class_hash_to_compiled_class_hash: HashMap<ClassHash, CompiledClassHash>,
}

#[derive(Debug, thiserror::Error)]
pub enum OsInputError {
    #[error("Invalid length of state readers: {0}. Should match size of block inputs: {1}")]
    InvalidLengthOfStateReaders(usize, usize),
}
