use starknet_api::test_utils::declare::DeclareTxArgs;
use starknet_api::transaction::TransactionHash;

use crate::execution::contract_class::ClassInfo;
use crate::transaction::account_transaction::AccountTransaction;
use crate::transaction::transactions::DeclareTransaction;

pub fn declare_tx(declare_tx_args: DeclareTxArgs, class_info: ClassInfo) -> AccountTransaction {
    let default_tx_hash = TransactionHash::default();
    let declare_tx = starknet_api::test_utils::declare::declare_tx(declare_tx_args);

    AccountTransaction::Declare(
        DeclareTransaction::new(declare_tx, default_tx_hash, class_info).unwrap(),
    )
}
