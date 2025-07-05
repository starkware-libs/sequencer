use std::fs;

use blockifier::abi::constants;
use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::blockifier::transaction_executor::TransactionExecutor;
use blockifier::blockifier_versioned_constants::VersionedConstants;
use blockifier::bouncer::BouncerConfig;
use blockifier::context::BlockContext;
use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::state::cached_state::{CommitmentStateDiff, StateMaps};
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{StateReader, StateResult};
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockInfo, BlockNumber, StarknetVersion};
use starknet_api::core::{ChainId, ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_api::transaction::{Transaction, TransactionHash};
use starknet_core::types::ContractClass as StarknetContractClass;
use starknet_types_core::felt::Felt;

use crate::state_reader::compile::{
    legacy_to_contract_class_v0,
    sierra_to_versioned_contract_class_v1,
};
use crate::state_reader::errors::ReexecutionResult;
use crate::state_reader::reexecution_state_reader::{
    ConsecutiveReexecutionStateReaders,
    ReexecutionStateReader,
};
use crate::state_reader::test_state_reader::StarknetContractClassMapping;
use crate::state_reader::utils::{get_chain_info, ReexecutionStateMaps};

pub struct OfflineReexecutionData {
    offline_state_reader_prev_block: OfflineStateReader,
    block_context_next_block: BlockContext,
    transactions_next_block: Vec<BlockifierTransaction>,
    state_diff_next_block: CommitmentStateDiff,
}

#[derive(Serialize, Deserialize)]
pub struct SerializableDataNextBlock {
    pub block_info_next_block: BlockInfo,
    pub starknet_version: StarknetVersion,
    pub transactions_next_block: Vec<(Transaction, TransactionHash)>,
    pub state_diff_next_block: CommitmentStateDiff,
    pub declared_classes: StarknetContractClassMapping,
}

#[derive(Serialize, Deserialize)]
pub struct SerializableDataPrevBlock {
    pub state_maps: ReexecutionStateMaps,
    pub contract_class_mapping: StarknetContractClassMapping,
}

#[derive(Serialize, Deserialize)]
pub struct SerializableOfflineReexecutionData {
    pub serializable_data_prev_block: SerializableDataPrevBlock,
    pub serializable_data_next_block: SerializableDataNextBlock,
    pub chain_id: ChainId,
    pub old_block_hash: BlockHash,
}

impl SerializableOfflineReexecutionData {
    pub fn write_to_file(&self, full_file_path: &str) -> ReexecutionResult<()> {
        let file_path = full_file_path.rsplit_once('/').expect("Invalid file path.").0;
        fs::create_dir_all(file_path)
            .unwrap_or_else(|err| panic!("Failed to create directory {file_path}. Error: {err}"));
        fs::write(full_file_path, serde_json::to_string_pretty(&self)?)
            .unwrap_or_else(|err| panic!("Failed to write to file {full_file_path}. Error: {err}"));
        Ok(())
    }

    pub fn read_from_file(full_file_path: &str) -> ReexecutionResult<Self> {
        let file_content = fs::read_to_string(full_file_path).unwrap_or_else(|err| {
            panic!("Failed to read reexecution data from file {full_file_path}. Error: {err}")
        });
        Ok(serde_json::from_str(&file_content)?)
    }
}

impl From<SerializableOfflineReexecutionData> for OfflineReexecutionData {
    fn from(value: SerializableOfflineReexecutionData) -> Self {
        let SerializableOfflineReexecutionData {
            serializable_data_prev_block:
                SerializableDataPrevBlock { state_maps, contract_class_mapping },
            serializable_data_next_block:
                SerializableDataNextBlock {
                    block_info_next_block,
                    starknet_version,
                    transactions_next_block,
                    state_diff_next_block,
                    declared_classes,
                },
            chain_id,
            old_block_hash,
        } = value;

        let offline_state_reader_prev_block = OfflineStateReader {
            state_maps: state_maps.try_into().expect("Failed to deserialize state maps."),
            contract_class_mapping,
            old_block_hash,
        };

        // Use the declared classes from the next block to allow retrieving the class info.
        let transactions_next_block =
            OfflineStateReader { contract_class_mapping: declared_classes, ..Default::default() }
                .api_txs_to_blockifier_txs_next_block(transactions_next_block)
                .expect("Failed to convert starknet-api transactions to blockifier transactions.");

        Self {
            offline_state_reader_prev_block,
            block_context_next_block: BlockContext::new(
                block_info_next_block,
                get_chain_info(&chain_id),
                VersionedConstants::get(&starknet_version).unwrap().clone(),
                BouncerConfig::max(),
            ),
            transactions_next_block,
            state_diff_next_block,
        }
    }
}

#[derive(Clone, Default)]
pub struct OfflineStateReader {
    pub state_maps: StateMaps,
    pub contract_class_mapping: StarknetContractClassMapping,
    pub old_block_hash: BlockHash,
}

impl StateReader for OfflineStateReader {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt> {
        Ok(*self.state_maps.storage.get(&(contract_address, key)).ok_or(
            StateError::StateReadError(format!(
                "Missing Storage Value at contract_address: {contract_address}, key:{key:?}"
            )),
        )?)
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        Ok(*self.state_maps.nonces.get(&contract_address).ok_or(StateError::StateReadError(
            format!("Missing nonce at contract_address: {contract_address}"),
        ))?)
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        Ok(*self.state_maps.class_hashes.get(&contract_address).ok_or(
            StateError::StateReadError(format!(
                "Missing class hash at contract_address: {contract_address}"
            )),
        )?)
    }

    fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        match self.get_contract_class(&class_hash)? {
            StarknetContractClass::Sierra(sierra) => {
                let (casm, _) = sierra_to_versioned_contract_class_v1(sierra).unwrap();
                Ok(casm.try_into().unwrap())
            }
            StarknetContractClass::Legacy(legacy) => {
                Ok(legacy_to_contract_class_v0(legacy).unwrap().try_into().unwrap())
            }
        }
    }

    fn get_compiled_class_hash(&self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        Ok(*self
            .state_maps
            .compiled_class_hashes
            .get(&class_hash)
            .ok_or(StateError::UndeclaredClassHash(class_hash))?)
    }
}

impl ReexecutionStateReader for OfflineStateReader {
    fn get_contract_class(&self, class_hash: &ClassHash) -> StateResult<StarknetContractClass> {
        Ok(self
            .contract_class_mapping
            .get(class_hash)
            .ok_or(StateError::UndeclaredClassHash(*class_hash))?
            .clone())
    }

    fn get_old_block_hash(&self, _old_block_number: BlockNumber) -> ReexecutionResult<BlockHash> {
        Ok(self.old_block_hash)
    }
}

impl OfflineStateReader {
    pub fn get_transaction_executor(
        self,
        block_context_next_block: BlockContext,
        transaction_executor_config: Option<TransactionExecutorConfig>,
    ) -> ReexecutionResult<TransactionExecutor<OfflineStateReader>> {
        let old_block_number = BlockNumber(
            block_context_next_block.block_info().block_number.0
                - constants::STORED_BLOCK_HASH_BUFFER,
        );
        let hash = self.old_block_hash;
        Ok(TransactionExecutor::<OfflineStateReader>::pre_process_and_create(
            self,
            block_context_next_block,
            Some(BlockHashAndNumber { number: old_block_number, hash }),
            transaction_executor_config.unwrap_or_default(),
        )?)
    }
}

pub struct OfflineConsecutiveStateReaders {
    pub offline_state_reader_prev_block: OfflineStateReader,
    pub block_context_next_block: BlockContext,
    pub transactions_next_block: Vec<BlockifierTransaction>,
    pub state_diff_next_block: CommitmentStateDiff,
}

impl OfflineConsecutiveStateReaders {
    pub fn new_from_file(full_file_path: &str) -> ReexecutionResult<Self> {
        let serializable_offline_reexecution_data =
            SerializableOfflineReexecutionData::read_from_file(full_file_path)?;
        Ok(Self::new(serializable_offline_reexecution_data.into()))
    }

    pub fn new(
        OfflineReexecutionData {
            offline_state_reader_prev_block,
            block_context_next_block,
            transactions_next_block,
            state_diff_next_block,
        }: OfflineReexecutionData,
    ) -> Self {
        Self {
            offline_state_reader_prev_block,
            block_context_next_block,
            transactions_next_block,
            state_diff_next_block,
        }
    }
}

impl ConsecutiveReexecutionStateReaders<OfflineStateReader> for OfflineConsecutiveStateReaders {
    fn pre_process_and_create_executor(
        self,
        transaction_executor_config: Option<TransactionExecutorConfig>,
    ) -> ReexecutionResult<TransactionExecutor<OfflineStateReader>> {
        self.offline_state_reader_prev_block
            .get_transaction_executor(self.block_context_next_block, transaction_executor_config)
    }

    fn get_next_block_txs(&self) -> ReexecutionResult<Vec<BlockifierTransaction>> {
        Ok(self.transactions_next_block.clone())
    }

    fn get_next_block_state_diff(&self) -> ReexecutionResult<CommitmentStateDiff> {
        Ok(self.state_diff_next_block.clone())
    }
}
