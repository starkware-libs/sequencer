use std::ops::Range;

use mempool_test_utils::starknet_api_test_utils::create_executable_tx;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::executable_transaction::Transaction;
use starknet_api::transaction::{DeprecatedResourceBoundsMapping, Tip, TransactionHash};
use starknet_types_core::felt::Felt;
use num_traits::cast::FromPrimitive;

pub fn test_txs(tx_hash_range: Range<usize>) -> Vec<Transaction> {
    tx_hash_range
        .map(|i| {
            create_executable_tx(
                ContractAddress::default(),
                TransactionHash(Felt::from_usize(i).unwrap()),
                Tip::default(),
                Nonce::default(),
                DeprecatedResourceBoundsMapping::default(),
            )
        })
        .collect()
}
