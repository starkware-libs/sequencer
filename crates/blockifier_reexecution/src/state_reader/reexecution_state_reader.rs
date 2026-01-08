use apollo_rpc_execution::DEPRECATED_CONTRACT_SIERRA_SIZE;
use assert_matches::assert_matches;
use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::blockifier::transaction_executor::TransactionExecutor;
use blockifier::state::cached_state::{CachedState, CommitmentStateDiff};
use blockifier::state::global_cache::CompiledClasses;
use blockifier::state::state_api::{StateReader, StateResult};
use blockifier::transaction::account_transaction::ExecutionFlags;
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::contract_class::{ClassInfo, SierraVersion};
use starknet_api::core::ClassHash;
use starknet_api::state::SierraContractClass;
use starknet_api::test_utils::MAX_FEE;
use starknet_api::transaction::{Transaction, TransactionHash};
use starknet_core::types::ContractClass as StarknetContractClass;

use crate::assert_eq_state_diff;
use crate::compile::{legacy_to_contract_class_v0, sierra_to_versioned_contract_class_v1};
use crate::errors::ReexecutionResult;
use crate::utils::contract_class_to_compiled_classes;

pub trait ReexecutionStateReader {
    fn get_contract_class(&self, class_hash: &ClassHash) -> StateResult<StarknetContractClass>;

    fn get_class_info(&self, class_hash: ClassHash) -> ReexecutionResult<ClassInfo> {
        match self.get_contract_class(&class_hash)? {
            StarknetContractClass::Sierra(sierra) => {
                let abi_length = sierra.abi.len();
                let sierra_length = sierra.sierra_program.len();

                let sierra_contract = SierraContractClass::from(sierra);
                let (contract_class, sierra_version) =
                    sierra_to_versioned_contract_class_v1(sierra_contract)?;

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

    fn get_old_block_hash(&self, old_block_number: BlockNumber) -> ReexecutionResult<BlockHash>;

    /// Converts a `starknet_core::types::ContractClass` to `CompiledClasses`.
    fn starknet_core_contract_class_to_compiled_class(
        &self,
        contract_class: StarknetContractClass,
    ) -> StateResult<CompiledClasses> {
        match contract_class {
            StarknetContractClass::Sierra(flat_sierra) => {
                let sierra = SierraContractClass::from(flat_sierra);
                let (class_v1, _) = sierra_to_versioned_contract_class_v1(sierra.clone())?;

                Ok(contract_class_to_compiled_classes(class_v1, Some(sierra))?)
            }
            StarknetContractClass::Legacy(legacy) => {
                let class_v0 = legacy_to_contract_class_v0(legacy)?;

                Ok(contract_class_to_compiled_classes(class_v0, None)?)
            }
        }
    }
}

/// Trait of the functions \ queries required for reexecution.
pub trait ConsecutiveReexecutionStateReaders<S: StateReader + Send + Sync + 'static>:
    Sized
{
    fn pre_process_and_create_executor(
        self,
        transaction_executor_config: Option<TransactionExecutorConfig>,
    ) -> ReexecutionResult<TransactionExecutor<S>>;

    fn get_next_block_txs(&self) -> ReexecutionResult<Vec<BlockifierTransaction>>;

    fn get_next_block_state_diff(&self) -> ReexecutionResult<CommitmentStateDiff>;

    /// Reexecutes a block and returns the block state along with the expected and actual state
    /// diffs. Does not verify that the state diffs match.
    fn reexecute_block(self) -> (Option<CachedState<S>>, CommitmentStateDiff, CommitmentStateDiff) {
        let expected_state_diff = self.get_next_block_state_diff().unwrap();

        let all_txs_in_next_block = self.get_next_block_txs().unwrap();

        let mut transaction_executor = self.pre_process_and_create_executor(None).unwrap();

        let execution_results = transaction_executor.execute_txs(&all_txs_in_next_block, None);
        // Verify all transactions executed successfully.
        for res in execution_results.iter() {
            assert_matches!(res, Ok(_));
        }

        // Finalize block and read actual statediff; using non_consuming_finalize to keep the
        // block_state.
        let actual_state_diff = transaction_executor
            .non_consuming_finalize()
            .expect("Couldn't finalize block")
            .state_diff;

        (transaction_executor.block_state, expected_state_diff, actual_state_diff)
    }

    fn reexecute_and_verify_correctness(self) -> Option<CachedState<S>> {
        let (block_state, expected_state_diff, actual_state_diff) = self.reexecute_block();

        assert_eq_state_diff!(expected_state_diff, actual_state_diff);

        block_state
    }
}
