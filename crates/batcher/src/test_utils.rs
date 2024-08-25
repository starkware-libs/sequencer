use std::ops::Range;

use mempool_test_utils::starknet_api_test_utils::create_executable_tx;
use num_traits::cast::FromPrimitive;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::executable_transaction::Transaction;
use starknet_api::transaction::{Tip, TransactionHash, ValidResourceBounds};
use starknet_types_core::felt::Felt;

pub fn test_txs(tx_hash_range: Range<usize>) -> Vec<Transaction> {
    tx_hash_range
        .map(|i| {
            create_executable_tx(
                ContractAddress::default(),
                TransactionHash(Felt::from_usize(i).unwrap()),
                Tip::default(),
                Nonce::default(),
                ValidResourceBounds::create_for_testing(),
            )
        })
        .collect()
}
