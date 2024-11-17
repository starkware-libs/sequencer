use starknet_api::contract_class::ClassInfo;
use starknet_api::test_utils::declare::{executable_declare_tx, DeclareTxArgs};

use crate::transaction::account_transaction::AccountTransaction;

pub fn declare_tx(declare_tx_args: DeclareTxArgs, class_info: ClassInfo) -> AccountTransaction {
    let declare_tx = executable_declare_tx(declare_tx_args, class_info);

    declare_tx.into()
}
