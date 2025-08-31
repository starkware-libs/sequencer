#![allow(dead_code)]
use blockifier::state::state_api::UpdatableState;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier::transaction::transaction_execution::Transaction;
use starknet_api::contract_class::ContractClass;
use starknet_api::executable_transaction::AccountTransaction;

pub(crate) trait FlowTestState: UpdatableState + Sync + Send + 'static {
    fn create_empty_state() -> Self;

    /// Hook to preprocess the state before executing the transactions.
    fn preprocess_before_execution(&mut self, txs: &[Transaction]);
}

impl FlowTestState for DictStateReader {
    fn create_empty_state() -> Self {
        DictStateReader::default()
    }

    /// Find all Cairo1 declares and explicitly add the compiled class hashes to the state, assuming
    /// they are blake (V2) hashes.
    /// When this trait is implemented on the [DictStateReader], it is required to store the V2
    /// hashes before executing the transactions, to indicate that migration is NOT required.
    fn preprocess_before_execution(&mut self, txs: &[Transaction]) {
        for tx in txs.iter() {
            if let Transaction::Account(account_tx) = tx {
                if let AccountTransaction::Declare(ref declare_tx) = account_tx.tx {
                    if let ContractClass::V1(_) = declare_tx.class_info.contract_class {
                        let class_hash = declare_tx.class_hash();
                        let compiled_class_hash = declare_tx.compiled_class_hash();
                        self.class_hash_to_compiled_class_hash_v2
                            .insert(class_hash, compiled_class_hash);
                    }
                }
            }
        }
    }
}
