use starknet_api::contract_class::ClassInfo;
use starknet_api::executable_transaction::{DeclareTransaction, Transaction};
use starknet_api::test_utils::declare::DeclareTxArgs;

use crate::transaction::account_transaction::AccountTransaction;

pub fn declare_tx(declare_tx_args: DeclareTxArgs, class_info: ClassInfo) -> AccountTransaction {
    let tx_hash = declare_tx_args.tx_hash;
    let declare_tx = starknet_api::test_utils::declare::declare_tx(declare_tx_args);

    let executable_tx =
        Transaction::Declare(DeclareTransaction { tx: declare_tx, tx_hash, class_info });

    AccountTransaction::new(executable_tx)
}
