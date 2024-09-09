use starknet_api::test_utils::invoke::InvokeTxArgs;

use crate::transaction::transactions::InvokeTransaction;

pub fn invoke_tx(invoke_args: InvokeTxArgs) -> InvokeTransaction {
    let tx_hash = invoke_args.tx_hash;
    let only_query = invoke_args.only_query;
    let invoke_tx = starknet_api::test_utils::invoke::invoke_tx(invoke_args);

    match only_query {
        true => InvokeTransaction::new_for_query(invoke_tx, tx_hash),
        false => InvokeTransaction::new(invoke_tx, tx_hash),
    }
}
