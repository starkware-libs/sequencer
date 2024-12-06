use starknet_api::contract_class::ClassInfo;
use starknet_api::executable_transaction::AccountTransaction;
use starknet_api::test_utils::declare::{executable_declare_tx, DeclareTxArgs};

// TODO(AvivG): remove this func & file.
pub fn declare_tx(declare_tx_args: DeclareTxArgs, class_info: ClassInfo) -> AccountTransaction {
    // TODO(AvivG): see into making 'executable_declare_tx' ret type AccountTransaction.
    let declare_tx = executable_declare_tx(declare_tx_args, class_info);

    AccountTransaction::Declare(declare_tx)
}
