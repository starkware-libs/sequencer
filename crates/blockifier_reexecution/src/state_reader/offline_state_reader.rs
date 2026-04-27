use std::fs;

use blockifier::abi::constants;
use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::blockifier::transaction_executor::TransactionExecutor;
use blockifier::blockifier_versioned_constants::VersionedConstants;
use blockifier::bouncer::BouncerConfig;
use blockifier::context::BlockContext;
use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::state::cached_state::{CommitmentStateDiff, StateMaps};
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::errors::StateError;
use blockifier::state::global_cache::CompiledClasses;
use blockifier::state::state_api::{StateReader, StateResult};
use blockifier::state::state_reader_and_contract_manager::{
    FetchCompiledClasses,
    StateReaderAndContractManager,
};
use blockifier::state::utils::get_compiled_class_hash_v2 as default_get_compiled_class_hash_v2;
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockInfo, BlockNumber, StarknetVersion};
use starknet_api::core::{ChainId, ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::{SierraContractClass, StorageKey};
use starknet_api::transaction::{Transaction, TransactionHash};
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;
use starknet_core::types::ContractClass as StarknetContractClass;
use starknet_types_core::felt::Felt;

use crate::compile::{legacy_to_contract_class_v0, sierra_to_versioned_contract_class_v1};
use crate::errors::ReexecutionResult;
use crate::state_reader::reexecution_state_reader::{BlockReexecutor, ReexecutionStateReader};
use crate::state_reader::rpc_state_reader::StarknetContractClassMapping;
use crate::utils::get_chain_info;

pub struct OfflineReexecutionData {
    base_block_state_reader: OfflineStateReader,
    reexecuted_block_context: BlockContext,
    reexecuted_block_transactions: Vec<BlockifierTransaction>,
    reexecuted_block_state_diff: CommitmentStateDiff,
}

#[derive(Serialize, Deserialize)]
pub struct SerializableDataReexecutedBlock {
    pub reexecuted_block_info: BlockInfo,
    pub starknet_version: StarknetVersion,
    pub reexecuted_block_transactions: Vec<(Transaction, TransactionHash)>,
    pub reexecuted_block_state_diff: CommitmentStateDiff,
    pub declared_classes: StarknetContractClassMapping,
}

#[derive(Serialize, Deserialize)]
pub struct SerializableDataBaseBlock {
    pub state_maps: StateMaps,
    pub contract_class_mapping: StarknetContractClassMapping,
}

#[derive(Serialize, Deserialize)]
pub struct SerializableOfflineReexecutionData {
    pub serializable_data_base_block: SerializableDataBaseBlock,
    pub serializable_data_reexecuted_block: SerializableDataReexecutedBlock,
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
            serializable_data_base_block:
                SerializableDataBaseBlock { state_maps, contract_class_mapping },
            serializable_data_reexecuted_block:
                SerializableDataReexecutedBlock {
                    reexecuted_block_info,
                    starknet_version,
                    reexecuted_block_transactions,
                    reexecuted_block_state_diff,
                    declared_classes,
                },
            chain_id,
            old_block_hash,
        } = value;

        let base_block_state_reader =
            OfflineStateReader { state_maps, contract_class_mapping, old_block_hash };

        // Use the declared classes from the reexecuted block to allow retrieving the class info.
        let reexecuted_block_transactions =
            OfflineStateReader { contract_class_mapping: declared_classes, ..Default::default() }
                .api_txs_to_blockifier_txs(reexecuted_block_transactions)
                .expect("Failed to convert starknet-api transactions to blockifier transactions.");

        let mut versioned_constants = VersionedConstants::get(&starknet_version).unwrap().clone();
        versioned_constants.disable_casm_hash_migration();

        Self {
            base_block_state_reader,
            reexecuted_block_context: BlockContext::new(
                reexecuted_block_info,
                get_chain_info(&chain_id, None),
                versioned_constants,
                BouncerConfig::max(),
            ),
            reexecuted_block_transactions,
            reexecuted_block_state_diff,
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
                let sierra_contract = SierraContractClass::from(sierra);
                let (casm, _) = sierra_to_versioned_contract_class_v1(sierra_contract).unwrap();
                Ok(casm.try_into().unwrap())
            }
            StarknetContractClass::Legacy(legacy) => {
                Ok(legacy_to_contract_class_v0(legacy).unwrap().try_into().unwrap())
            }
        }
    }

    fn get_compiled_class_hash(&self, _class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        unimplemented!("The offline state reader does not support get_compiled_class_hash.")
    }

    fn get_compiled_class_hash_v2(
        &self,
        class_hash: ClassHash,
        compiled_class: &RunnableCompiledClass,
    ) -> StateResult<CompiledClassHash> {
        default_get_compiled_class_hash_v2(self, class_hash, compiled_class)
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

impl FetchCompiledClasses for OfflineStateReader {
    fn get_compiled_classes(&self, class_hash: ClassHash) -> StateResult<CompiledClasses> {
        let contract_class = self.get_contract_class(&class_hash)?;
        self.starknet_core_contract_class_to_compiled_class(contract_class)
    }

    /// This check is no needed in the reexecution context.
    /// We assume that all the classes returned successfuly by the OfflineStateReader are declared.
    fn is_declared(&self, _class_hash: ClassHash) -> StateResult<bool> {
        Ok(true)
    }
}

impl OfflineStateReader {
    pub fn get_transaction_executor(
        self,
        reexecuted_block_context: BlockContext,
        transaction_executor_config: Option<TransactionExecutorConfig>,
        contract_class_manager: &ContractClassManager,
    ) -> ReexecutionResult<TransactionExecutor<StateReaderAndContractManager<OfflineStateReader>>>
    {
        let old_block_number = BlockNumber(
            reexecuted_block_context.block_info().block_number.0
                - constants::STORED_BLOCK_HASH_BUFFER,
        );
        let hash = self.old_block_hash;
        // We don't collect class cache metrics for the reexecution.
        let class_cache_metrics = None;
        let state_reader_and_contract_manager = StateReaderAndContractManager::new(
            self,
            contract_class_manager.clone(),
            class_cache_metrics,
        );
        Ok(TransactionExecutor::<StateReaderAndContractManager<OfflineStateReader>>::pre_process_and_create(
            state_reader_and_contract_manager,
            reexecuted_block_context,
            Some(BlockHashAndNumber { number: old_block_number, hash }),
            transaction_executor_config.unwrap_or_default(),
        )?)
    }
}

pub struct OfflineBlockReexecutor {
    pub base_block_state_reader: OfflineStateReader,
    pub reexecuted_block_context: BlockContext,
    pub reexecuted_block_transactions: Vec<BlockifierTransaction>,
    pub reexecuted_block_state_diff: CommitmentStateDiff,
    contract_class_manager: ContractClassManager,
}

impl OfflineBlockReexecutor {
    pub fn new_from_file(
        full_file_path: &str,
        contract_class_manager: ContractClassManager,
    ) -> ReexecutionResult<Self> {
        let serializable_offline_reexecution_data =
            SerializableOfflineReexecutionData::read_from_file(full_file_path)?;
        Ok(Self::new(serializable_offline_reexecution_data.into(), contract_class_manager))
    }

    pub fn new(
        OfflineReexecutionData {
            base_block_state_reader,
            reexecuted_block_context,
            reexecuted_block_transactions,
            reexecuted_block_state_diff,
        }: OfflineReexecutionData,
        contract_class_manager: ContractClassManager,
    ) -> Self {
        Self {
            base_block_state_reader,
            reexecuted_block_context,
            reexecuted_block_transactions,
            reexecuted_block_state_diff,
            contract_class_manager,
        }
    }
}

impl BlockReexecutor<StateReaderAndContractManager<OfflineStateReader>> for OfflineBlockReexecutor {
    fn pre_process_and_create_executor(
        self,
        transaction_executor_config: Option<TransactionExecutorConfig>,
    ) -> ReexecutionResult<TransactionExecutor<StateReaderAndContractManager<OfflineStateReader>>>
    {
        self.base_block_state_reader.get_transaction_executor(
            self.reexecuted_block_context,
            transaction_executor_config,
            &self.contract_class_manager,
        )
    }

    fn get_block_txs(&self) -> ReexecutionResult<Vec<BlockifierTransaction>> {
        Ok(self.reexecuted_block_transactions.clone())
    }

    fn get_block_state_diff(&self) -> ReexecutionResult<CommitmentStateDiff> {
        Ok(self.reexecuted_block_state_diff.clone())
    }
}
