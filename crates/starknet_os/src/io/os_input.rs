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
use starknet_api::state::StorageKey;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::types::SubTreeHeight;
use starknet_types_core::felt::Felt;
use tracing::level_filters::LevelFilter;

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
#[derive(Debug)]
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
    contract_class_version: Felt,
    external_functions_hash: HashOutput,
    l1_handlers_hash: HashOutput,
    constructors_hash: HashOutput,
    abi_hash: HashOutput,
    sierra_program_hash: HashOutput,
}

impl ContractClassComponentHashes {
    pub(crate) fn flatten(&self) -> Vec<Felt> {
        vec![
            self.contract_class_version,
            self.external_functions_hash.0,
            self.l1_handlers_hash.0,
            self.constructors_hash.0,
            self.abi_hash.0,
            self.sierra_program_hash.0,
        ]
    }
}

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
#[cfg_attr(any(test, feature = "testing"), derive(Default))]
#[derive(Debug)]
pub struct OsHints {
    pub os_input: StarknetOsInput,
    pub os_hints_config: OsHintsConfig,
}

// TODO(Dori): Once computation of the hinted class hash is fully functional, delete this type.
pub(crate) type HintedClassHash = Felt;

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
#[cfg_attr(any(test, feature = "testing"), derive(Default))]
#[derive(Debug)]
pub struct StarknetOsInput {
    pub os_block_inputs: Vec<OsBlockInput>,
    pub cached_state_inputs: Vec<CachedStateInput>,
    // TODO(Dori): Once computation of the hinted class hash is fully functional, the extra Felt
    //   value in the tuple should be removed.
    pub(crate) deprecated_compiled_classes: BTreeMap<ClassHash, (HintedClassHash, ContractClass)>,
    pub(crate) compiled_classes: BTreeMap<ClassHash, CasmContractClass>,
}

// TODO(Meshi): Remove Once the blockifier ChainInfo do not support deprecated fee token.
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OsChainInfo {
    #[cfg_attr(feature = "deserialize", serde(deserialize_with = "deserialize_chain_id_from_hex"))]
    pub(crate) chain_id: ChainId,
    pub(crate) strk_fee_token_address: ContractAddress,
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
#[cfg_attr(any(test, feature = "testing"), derive(Default))]
#[derive(Debug)]
pub struct OsBlockInput {
    pub(crate) contract_state_commitment_info: CommitmentInfo,
    pub(crate) address_to_storage_commitment_info: HashMap<ContractAddress, CommitmentInfo>,
    pub(crate) contract_class_commitment_info: CommitmentInfo,
    // Note: The Declare tx in the starknet_api crate has a class_info field with a contract_class
    // field. This field is needed by the blockifier, but not used in the OS, so it is expected
    // (and verified) to be initialized with an illegal value, to avoid using it accidentally.
    pub transactions: Vec<Transaction>,
    pub tx_execution_infos: Vec<CentralTransactionExecutionInfo>,
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
pub struct CachedStateInput {
    pub(crate) storage: HashMap<ContractAddress, HashMap<StorageKey, Felt>>,
    pub(crate) address_to_class_hash: HashMap<ContractAddress, ClassHash>,
    pub(crate) address_to_nonce: HashMap<ContractAddress, Nonce>,
    pub(crate) class_hash_to_compiled_class_hash: HashMap<ClassHash, CompiledClassHash>,
}

#[derive(Debug, thiserror::Error)]
pub enum OsInputError {
    #[error("Invalid length of state readers: {0}. Should match size of block inputs: {1}")]
    InvalidLengthOfStateReaders(usize, usize),
}
