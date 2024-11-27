use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::blockifier::transaction_executor::TransactionExecutor;
use blockifier::state::cached_state::CommitmentStateDiff;
use blockifier::state::state_api::{StateReader, StateResult};
use blockifier::test_utils::MAX_FEE;
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use papyrus_execution::DEPRECATED_CONTRACT_SIERRA_SIZE;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::contract_class::ClassInfo;
use starknet_api::core::ClassHash;
use starknet_api::transaction::{Transaction, TransactionHash};
use starknet_core::types::ContractClass as StarknetContractClass;

use crate::state_reader::compile::{legacy_to_contract_class_v0, sierra_to_contact_class_v1};
use crate::state_reader::errors::{ReexecutionError, ReexecutionResult};

pub trait ReexecutionStateReader {
    fn get_contract_class(&self, class_hash: &ClassHash) -> StateResult<StarknetContractClass>;

    fn get_class_info(&self, class_hash: ClassHash) -> ReexecutionResult<ClassInfo> {
        match self.get_contract_class(&class_hash)? {
            StarknetContractClass::Sierra(sierra) => {
                let abi_length = sierra.abi.len();
                let sierra_length = sierra.sierra_program.len();
                Ok(ClassInfo::new(&sierra_to_contact_class_v1(sierra)?, sierra_length, abi_length)?)
            }
            StarknetContractClass::Legacy(legacy) => {
                let abi_length =
                    legacy.abi.clone().expect("legendary contract should have abi").len();
                Ok(ClassInfo::new(
                    &legacy_to_contract_class_v0(legacy)?,
                    DEPRECATED_CONTRACT_SIERRA_SIZE,
                    abi_length,
                )?)
            }
        }
    }

    // TODO(Aner): extract this function out of the state reader.
    fn api_txs_to_blockifier_txs_next_block(
        &self,
        txs_and_hashes: Vec<(Transaction, TransactionHash)>,
    ) -> ReexecutionResult<Vec<BlockifierTransaction>> {
        txs_and_hashes
            .into_iter()
            .map(|(tx, tx_hash)| match tx {
                Transaction::Invoke(_) | Transaction::DeployAccount(_) => {
                    Ok(BlockifierTransaction::from_api(tx, tx_hash, None, None, None, false)?)
                }
                Transaction::Declare(ref declare_tx) => {
                    let class_info = self
                        .get_class_info(declare_tx.class_hash())
                        .map_err(ReexecutionError::from)?;
                    Ok(BlockifierTransaction::from_api(
                        tx,
                        tx_hash,
                        Some(class_info),
                        None,
                        None,
                        false,
                    )?)
                }
                Transaction::L1Handler(_) => Ok(BlockifierTransaction::from_api(
                    tx,
                    tx_hash,
                    None,
                    Some(MAX_FEE),
                    None,
                    false,
                )?),

                Transaction::Deploy(_) => {
                    panic!("Reexecution not supported for Deploy transactions.")
                }
            })
            .collect()
    }

    fn get_old_block_hash(&self, old_block_number: BlockNumber) -> ReexecutionResult<BlockHash>;
}

/// Trait of the functions \ queries required for reexecution.
pub trait ConsecutiveReexecutionStateReaders<S: StateReader> {
    fn pre_process_and_create_executor(
        self,
        transaction_executor_config: Option<TransactionExecutorConfig>,
    ) -> ReexecutionResult<TransactionExecutor<S>>;

    fn get_next_block_txs(&self) -> ReexecutionResult<Vec<BlockifierTransaction>>;

    fn get_next_block_state_diff(&self) -> ReexecutionResult<CommitmentStateDiff>;
}
