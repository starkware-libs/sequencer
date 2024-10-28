use starknet_api::test_utils::declare::DeclareTxArgs;

use crate::execution::contract_class::ClassInfo;
use crate::transaction::account_transaction::AccountTransaction;
use crate::transaction::transactions::DeclareTransaction;

pub fn declare_tx(declare_tx_args: DeclareTxArgs, class_info: ClassInfo) -> AccountTransaction {
    let tx_hash = declare_tx_args.tx_hash;
    let declare_tx = starknet_api::test_utils::declare::declare_tx(declare_tx_args);
    let executable_declare = DeclareTransaction::new(declare_tx, tx_hash, class_info).unwrap();

    executable_declare.into()
}
