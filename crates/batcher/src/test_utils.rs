use std::ops::Range;

// TODO(Yael 19/9/2024): move this function to starknet-api test utils
use mempool_test_utils::starknet_api_test_utils::create_executable_tx;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::executable_transaction::Transaction;
use starknet_api::felt;
use starknet_api::transaction::{ResourceBounds, Tip, TransactionHash, ValidResourceBounds};

pub fn test_txs(tx_hash_range: Range<usize>) -> Vec<Transaction> {
    tx_hash_range
        .map(|i| {
            create_executable_tx(
                ContractAddress::default(),
                TransactionHash(felt!(u128::try_from(i).unwrap())),
                Tip::default(),
                Nonce::default(),
                ValidResourceBounds::L1Gas(ResourceBounds::default()),
            )
        })
        .collect()
}
