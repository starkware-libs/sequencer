use starknet_api::executable_transaction::AccountTransaction as ExecutableTransaction;
use starknet_api::test_utils::invoke::InvokeTxArgs;

use crate::transaction::account_transaction::AccountTransaction;

pub fn invoke_tx(invoke_args: InvokeTxArgs) -> AccountTransaction {
    let only_query = invoke_args.only_query;
    let invoke_tx = ExecutableTransaction::Invoke(
        starknet_api::test_utils::invoke::executable_invoke_tx(invoke_args),
    );

    match only_query {
        true => AccountTransaction::new_for_query(invoke_tx),
        false => AccountTransaction::new(invoke_tx),
    }
}
