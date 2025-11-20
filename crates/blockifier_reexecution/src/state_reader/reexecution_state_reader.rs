use apollo_rpc_execution::DEPRECATED_CONTRACT_SIERRA_SIZE;
use blockifier::abi::constants;
use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::blockifier::transaction_executor::TransactionExecutor;
use blockifier::context::BlockContext;
use blockifier::state::cached_state::CommitmentStateDiff;
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::state_api::{StateReader, StateResult};
use blockifier::state::state_reader_and_contract_manager::{
    FetchCompiledClasses,
    StateReaderAndContractManager,
};
use blockifier::transaction::account_transaction::ExecutionFlags;
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockNumber};
use starknet_api::contract_class::{ClassInfo, SierraVersion};
use starknet_api::core::ClassHash;
use starknet_api::test_utils::MAX_FEE;
use starknet_api::transaction::{Transaction, TransactionHash};
use starknet_core::types::ContractClass as StarknetContractClass;

use crate::state_reader::compile::{
    legacy_to_contract_class_v0,
    sierra_to_versioned_contract_class_v1,
};
use crate::state_reader::errors::ReexecutionResult;

pub trait ReexecutionStateReader {
    fn get_contract_class(&self, class_hash: &ClassHash) -> StateResult<StarknetContractClass>;

    fn get_class_info(&self, class_hash: ClassHash) -> ReexecutionResult<ClassInfo> {
        match self.get_contract_class(&class_hash)? {
            StarknetContractClass::Sierra(sierra) => {
                let abi_length = sierra.abi.len();
                let sierra_length = sierra.sierra_program.len();

                let (contract_class, sierra_version) =
                    sierra_to_versioned_contract_class_v1(sierra)?;

                Ok(ClassInfo::new(&contract_class, sierra_length, abi_length, sierra_version)?)
            }
            StarknetContractClass::Legacy(legacy) => {
                let abi_length =
                    legacy.abi.clone().expect("legendary contract should have abi").len();
                Ok(ClassInfo::new(
                    &legacy_to_contract_class_v0(legacy)?,
                    DEPRECATED_CONTRACT_SIERRA_SIZE,
                    abi_length,
                    SierraVersion::DEPRECATED,
                )?)
            }
        }
    }

    // TODO(Aner): extract this function out of the state reader.
    fn api_txs_to_blockifier_txs_next_block(
        &self,
        txs_and_hashes: Vec<(Transaction, TransactionHash)>,
    ) -> ReexecutionResult<Vec<BlockifierTransaction>> {
        let execution_flags = ExecutionFlags::default();
        txs_and_hashes
            .into_iter()
            .map(|(tx, tx_hash)| match tx {
                Transaction::Invoke(_) | Transaction::DeployAccount(_) => {
                    Ok(BlockifierTransaction::from_api(
                        tx,
                        tx_hash,
                        None,
                        None,
                        None,
                        execution_flags.clone(),
                    )?)
                }
                Transaction::Declare(ref declare_tx) => {
                    let class_info = self.get_class_info(declare_tx.class_hash())?;
                    Ok(BlockifierTransaction::from_api(
                        tx,
                        tx_hash,
                        Some(class_info),
                        None,
                        None,
                        execution_flags.clone(),
                    )?)
                }
                Transaction::L1Handler(_) => Ok(BlockifierTransaction::from_api(
                    tx,
                    tx_hash,
                    None,
                    Some(MAX_FEE),
                    None,
                    execution_flags.clone(),
                )?),

                Transaction::Deploy(_) => {
                    panic!("Reexecution not supported for Deploy transactions.")
                }
            })
            .collect()
    }

    fn get_transaction_executor(
        self,
        block_context_next_block: BlockContext,
        transaction_executor_config: Option<TransactionExecutorConfig>,
        contract_class_manager: &ContractClassManager,
    ) -> ReexecutionResult<TransactionExecutor<StateReaderAndContractManager<Self>>>
    where
        Self: StateReader + FetchCompiledClasses + Sized,
    {
        let old_block_number = BlockNumber(
            block_context_next_block.block_info().block_number.0
                - constants::STORED_BLOCK_HASH_BUFFER,
        );
        let old_block_hash = self.get_old_block_hash(old_block_number)?;

        let state_reader = StateReaderAndContractManager {
            state_reader: self,
            contract_class_manager: contract_class_manager.clone(),
        };
        Ok(TransactionExecutor::<StateReaderAndContractManager<Self>>::pre_process_and_create(
            state_reader,
            block_context_next_block,
            Some(BlockHashAndNumber { number: old_block_number, hash: old_block_hash }),
            transaction_executor_config.unwrap_or_default(),
        )?)
    }

    fn get_old_block_hash(&self, old_block_number: BlockNumber) -> ReexecutionResult<BlockHash>;
}

/// Trait of the functions \ queries required for reexecution.
pub trait ConsecutiveReexecutionStateReaders<S: StateReader> {
    fn pre_process_and_create_executor(
        self,
        transaction_executor_config: Option<TransactionExecutorConfig>,
        contract_class_manager: &ContractClassManager,
    ) -> ReexecutionResult<TransactionExecutor<S>>;

    fn get_next_block_txs(&self) -> ReexecutionResult<Vec<BlockifierTransaction>>;

    fn get_next_block_state_diff(&self) -> ReexecutionResult<CommitmentStateDiff>;
}
