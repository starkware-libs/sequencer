use std::collections::HashMap;
use std::env;
use std::fs::read_to_string;
use std::path::Path;

use serde::{Deserialize, Serialize};
use starknet_types_core::felt::Felt;

use crate::block::BlockNumber;
use crate::core::{ChainId, ContractAddress, Nonce};
use crate::transaction::{Transaction, TransactionHash};

pub mod declare;
pub mod deploy_account;
pub mod invoke;

#[derive(Debug, Default)]
pub struct NonceManager {
    next_nonce: HashMap<ContractAddress, Felt>,
}

/// Reads from the directory containing the manifest at run time, same as current working directory.
pub fn read_json_file(path_in_resource_dir: &str) -> serde_json::Value {
    let path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("resources")
        .join(path_in_resource_dir);
    let json_str = read_to_string(path.to_str().unwrap())
        .unwrap_or_else(|_| panic!("Failed to read file at path: {}", path.display()));
    serde_json::from_str(&json_str).unwrap()
}

#[derive(Deserialize, Serialize)]
/// A struct used for reading the transaction test data (e.g., for transaction hash tests).
pub struct TransactionTestData {
    /// The actual transaction.
    pub transaction: Transaction,
    /// The expected transaction hash.
    pub transaction_hash: TransactionHash,
    /// An optional transaction hash to query.
    pub only_query_transaction_hash: Option<TransactionHash>,
    /// The chain ID.
    pub chain_id: ChainId,
    /// The block number.
    pub block_number: BlockNumber,
}

impl NonceManager {
    pub fn next(&mut self, account_address: ContractAddress) -> Nonce {
        let next = self.next_nonce.remove(&account_address).unwrap_or_default();
        self.next_nonce.insert(account_address, next + 1);
        Nonce(next)
    }

    /// Decrements the nonce of the account, unless it is zero.
    pub fn rollback(&mut self, account_address: ContractAddress) {
        let current = *self.next_nonce.get(&account_address).unwrap_or(&Felt::default());
        if current != Felt::ZERO {
            self.next_nonce.insert(account_address, current - 1);
        }
    }
}

/// A utility macro to create a [`Nonce`] from a hex string / unsigned integer
/// representation.
#[macro_export]
macro_rules! nonce {
    ($s:expr) => {
        $crate::core::Nonce(starknet_types_core::felt::Felt::from($s))
    };
}

/// A utility macro to create a [`StorageKey`](crate::state::StorageKey) from a hex string /
/// unsigned integer representation.
#[macro_export]
macro_rules! storage_key {
    ($s:expr) => {
        $crate::state::StorageKey(starknet_api::patricia_key!($s))
    };
}

/// A utility macro to create a [`CompiledClassHash`](crate::core::CompiledClassHash) from a hex
/// string / unsigned integer representation.
#[macro_export]
macro_rules! compiled_class_hash {
    ($s:expr) => {
        $crate::core::CompiledClassHash(starknet_types_core::felt::Felt::from($s))
    };
}
