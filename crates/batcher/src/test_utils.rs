use std::ops::Range;

use starknet_api::executable_transaction::Transaction;
use starknet_api::felt;
use starknet_api::test_utils::invoke::{executable_invoke_tx, InvokeTxArgs};
use starknet_api::transaction::TransactionHash;

pub fn test_txs(tx_hash_range: Range<usize>) -> Vec<Transaction> {
    tx_hash_range
        .map(|i| {
            Transaction::Invoke(executable_invoke_tx(InvokeTxArgs {
                tx_hash: TransactionHash(felt!(u128::try_from(i).unwrap())),
                ..Default::default()
            }))
        })
        .collect()
}
