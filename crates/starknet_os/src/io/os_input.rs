use std::collections::{BTreeMap, HashMap};

use blockifier::state::cached_state::StateMaps;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use shared_execution_objects::central_objects::CentralTransactionExecutionInfo;
use starknet_api::block::{BlockHash, BlockInfo, BlockNumber};
use starknet_api::block_hash::block_hash_calculator::BlockHeaderCommitments;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, OsChainInfo};
use starknet_api::deprecated_contract_class::ContractClass;
use starknet_api::executable_transaction::Transaction;
use starknet_api::state::ContractClassComponentHashes;
use starknet_types_core::felt::Felt;
use tracing::level_filters::LevelFilter;

use crate::commitment_infos::CommitmentInfo;

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
    pub deprecated_compiled_classes: BTreeMap<ClassHash, ContractClass>,
    pub compiled_classes: BTreeMap<CompiledClassHash, CasmContractClass>,
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
    pub block_hash_commitments: BlockHeaderCommitments,
    pub prev_block_hash: BlockHash,
    pub new_block_hash: BlockHash,
    // The block number and block hash of the (current_block_number - buffer) block, where
    // buffer=STORED_BLOCK_HASH_BUFFER.
    // It is the hash that is going to be written by this OS run.
    pub old_block_number_and_hash: Option<(BlockNumber, BlockHash)>,
    // A list of (class hash, compiled class hash v2) for all classes that require migration.
    pub class_hashes_to_migrate: Vec<(ClassHash, CompiledClassHash)>,
    // The initial reads of the block.
    pub initial_reads: StateMaps,
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
    pub public_keys: Option<Vec<Felt>>,
    pub rng_seed_salt: Option<Felt>,
}
impl OsHintsConfig {
    pub fn log_level(&self) -> LevelFilter {
        if self.debug_mode { LevelFilter::DEBUG } else { LevelFilter::INFO }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OsInputError {
    #[error("Invalid length of state readers: {0}. Should match size of block inputs: {1}")]
    InvalidLengthOfStateReaders(usize, usize),
}
