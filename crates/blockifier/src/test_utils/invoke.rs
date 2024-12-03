use starknet_api::executable_transaction::AccountTransaction as ExecutableTransaction;
use starknet_api::test_utils::invoke::{executable_invoke_tx, InvokeTxArgs};

use crate::transaction::account_transaction::AccountTransaction;

pub fn invoke_tx(invoke_args: InvokeTxArgs) -> AccountTransaction {
    let only_query = invoke_args.only_query;
    let invoke_tx = ExecutableTransaction::Invoke(executable_invoke_tx(invoke_args));

    AccountTransaction { tx: invoke_tx, only_query }
}
