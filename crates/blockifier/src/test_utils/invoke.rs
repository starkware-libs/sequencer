use starknet_api::executable_transaction::{
    InvokeTransaction as ExecutableInvokeTransaction,
    Transaction as ExecutableTransaction,
};
use starknet_api::test_utils::invoke::InvokeTxArgs;
use starknet_api::transaction::{InvokeTransaction, InvokeTransactionV0, TransactionVersion};

use crate::abi::abi_utils::selector_from_name;
use crate::transaction::account_transaction::AccountTransaction;
use crate::transaction::constants::EXECUTE_ENTRY_POINT_NAME;

pub fn invoke_tx(invoke_args: InvokeTxArgs) -> AccountTransaction {
    let tx_hash = invoke_args.tx_hash;
    let only_query = invoke_args.only_query;
    // TODO: Make TransactionVersion an enum and use match here.
    let invoke_tx = if invoke_args.version == TransactionVersion::ZERO {
        InvokeTransaction::V0(InvokeTransactionV0 {
            max_fee: invoke_args.max_fee,
            calldata: invoke_args.calldata,
            contract_address: invoke_args.sender_address,
            signature: invoke_args.signature,
            // V0 transactions should always select the `__execute__` entry point.
            entry_point_selector: selector_from_name(EXECUTE_ENTRY_POINT_NAME),
        })
    } else {
        starknet_api::test_utils::invoke::invoke_tx(invoke_args)
    };

    let invoke_tx =
        ExecutableTransaction::Invoke(ExecutableInvokeTransaction { tx: invoke_tx, tx_hash });

    match only_query {
        true => AccountTransaction::new_for_query(invoke_tx),
        false => AccountTransaction::new(invoke_tx),
    }
}
