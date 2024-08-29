use starknet_api::test_utils::declare::DeclareTxArgs;

use crate::execution::contract_class::ClassInfo;
use crate::transaction::account_transaction::AccountTransaction;
use crate::transaction::transactions::DeclareTransaction;

pub fn declare_tx(declare_tx_args: DeclareTxArgs, class_info: ClassInfo) -> AccountTransaction {
    let tx_hash = declare_tx_args.tx_hash;
    AccountTransaction::Declare(
        DeclareTransaction::new(
            starknet_api::test_utils::declare::declare_tx(declare_tx_args),
            tx_hash,
            class_info,
        )
        .unwrap(),
    )
}
