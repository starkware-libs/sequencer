use starknet_api::executable_transaction::AccountTransaction;
use starknet_api::test_utils::invoke::{executable_invoke_tx, InvokeTxArgs};

pub fn invoke_tx(invoke_args: InvokeTxArgs) -> AccountTransaction {
    AccountTransaction::Invoke(executable_invoke_tx(invoke_args))
}
