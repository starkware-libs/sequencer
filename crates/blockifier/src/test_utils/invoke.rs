use starknet_api::executable_transaction::AccountTransaction;
use starknet_api::test_utils::invoke::{executable_invoke_tx, InvokeTxArgs};

// TODO(AvivG): remove this func & file.
pub fn invoke_tx(invoke_args: InvokeTxArgs) -> AccountTransaction {
    // TODO(AvivG): see into making 'executable_invoke_tx' ret type AccountTransaction.
    AccountTransaction::Invoke(executable_invoke_tx(invoke_args))
}
