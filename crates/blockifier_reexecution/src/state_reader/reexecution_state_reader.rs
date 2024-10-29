use blockifier::execution::contract_class::ClassInfo;
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use starknet_api::core::ClassHash;
use starknet_api::transaction::{Transaction, TransactionHash};

use crate::state_reader::errors::ReexecutionError;
use crate::state_reader::test_state_reader::ReexecutionResult;

pub(crate) trait ReexecutionStateReader {
    fn get_class_info(&self, class_hash: ClassHash) -> ReexecutionResult<ClassInfo>;

    // TODO(Aner): extend/refactor to accomodate all types of transactions.
    fn api_txs_to_blockifier_txs(
        &self,
        txs_and_hashes: Vec<(Transaction, TransactionHash)>,
    ) -> ReexecutionResult<Vec<BlockifierTransaction>> {
        txs_and_hashes
            .into_iter()
            .map(|(tx, tx_hash)| match tx {
                Transaction::Invoke(_) | Transaction::DeployAccount(_) => {
                    BlockifierTransaction::from_api(tx, tx_hash, None, None, None, false)
                        .map_err(ReexecutionError::from)
                }
                Transaction::Declare(ref declare_tx) => {
                    let class_info = self
                        .get_class_info(declare_tx.class_hash())
                        .map_err(ReexecutionError::from)?;
                    BlockifierTransaction::from_api(
                        tx,
                        tx_hash,
                        Some(class_info),
                        None,
                        None,
                        false,
                    )
                    .map_err(ReexecutionError::from)
                }
                _ => unimplemented!("unimplemented transaction type: {:?}", tx),
            })
            .collect::<Result<Vec<_>, _>>()
    }
}
