use starknet_api::executable_transaction::{
    AccountTransaction as ExecutableTransaction,
    InvokeTransaction as ExecutableInvokeTransaction,
};
use starknet_api::test_utils::invoke::InvokeTxArgs;

use crate::transaction::account_transaction::AccountTransaction;

pub fn invoke_tx(invoke_args: InvokeTxArgs) -> AccountTransaction {
    let tx_hash = invoke_args.tx_hash;
    let only_query = invoke_args.only_query;
    let invoke_tx = starknet_api::test_utils::invoke::invoke_tx(invoke_args);

    let invoke_tx =
        ExecutableTransaction::Invoke(ExecutableInvokeTransaction { tx: invoke_tx, tx_hash });

    match only_query {
        true => AccountTransaction::new_for_query(invoke_tx),
        false => AccountTransaction::new(invoke_tx),
    }
}
