use blockifier::state::state_api::StateResult;
use blockifier::test_utils::MAX_FEE;
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use papyrus_execution::DEPRECATED_CONTRACT_SIERRA_SIZE;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::contract_class::{ClassInfo, SierraVersion};
use starknet_api::core::ClassHash;
use starknet_api::transaction::{Transaction, TransactionHash};
use starknet_core::types::ContractClass as StarknetContractClass;

use super::compile::{legacy_to_contract_class_v0, sierra_to_contact_class_v1};
use crate::state_reader::errors::ReexecutionError;
use crate::state_reader::test_state_reader::ReexecutionResult;

pub trait ReexecutionStateReader {
    fn get_contract_class(&self, class_hash: &ClassHash) -> StateResult<StarknetContractClass>;

    fn get_class_info(&self, class_hash: ClassHash) -> ReexecutionResult<ClassInfo> {
        match self.get_contract_class(&class_hash)? {
            StarknetContractClass::Sierra(sierra) => {
                let abi_length = sierra.abi.len();
                let sierra_length = sierra.sierra_program.len();

                // Extract the version from the sierra program.
                let (minor, major, path): (u64, u64, u64) = (
                    sierra.sierra_program[0].try_into().unwrap(),
                    sierra.sierra_program[1].try_into().unwrap(),
                    sierra.sierra_program[2].try_into().unwrap(),
                );
                let sierra_version: SierraVersion = SierraVersion::new(major, minor, path);

                Ok(ClassInfo::new(
                    &sierra_to_contact_class_v1(sierra)?,
                    sierra_length,
                    abi_length,
                    sierra_version,
                )?)
            }
            StarknetContractClass::Legacy(legacy) => {
                let abi_length =
                    legacy.abi.clone().expect("legendary contract should have abi").len();
                Ok(ClassInfo::new(
                    &legacy_to_contract_class_v0(legacy)?,
                    DEPRECATED_CONTRACT_SIERRA_SIZE,
                    abi_length,
                    SierraVersion::zero(),
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
