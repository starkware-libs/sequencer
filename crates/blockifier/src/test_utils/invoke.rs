use starknet_api::test_utils::invoke::InvokeTxArgs;

use crate::transaction::transactions::InvokeTransaction;

pub fn invoke_tx(invoke_args: InvokeTxArgs) -> InvokeTransaction {
    let only_query = invoke_args.only_query;
    // TODO: Make TransactionVersion an enum and use match here.
    let invoke_tx = starknet_api::test_utils::invoke::executable_invoke_tx(invoke_args);

    InvokeTransaction { tx: invoke_tx, only_query }
}
