use starknet_api::test_utils::invoke::InvokeTxArgs;
use starknet_api::transaction::TransactionHash;

use crate::transaction::transactions::InvokeTransaction;

pub fn invoke_tx(invoke_args: InvokeTxArgs) -> InvokeTransaction {
    let default_tx_hash = TransactionHash::default();
    let only_query = invoke_args.only_query;
    // TODO: Make TransactionVersion an enum and use match here.
    let invoke_tx = starknet_api::test_utils::invoke::invoke_tx(invoke_args);

    match only_query {
        true => InvokeTransaction::new_for_query(invoke_tx, default_tx_hash),
        false => InvokeTransaction::new(invoke_tx, default_tx_hash),
    }
}
