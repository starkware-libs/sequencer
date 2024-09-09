use std::collections::HashMap;

use starknet_types_core::felt::Felt;

use crate::core::{ContractAddress, Nonce};

pub mod declare;
pub mod deploy_account;
pub mod invoke;

#[derive(Default)]
pub struct NonceManager {
    next_nonce: HashMap<ContractAddress, Felt>,
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

/// A utility macro to create a [`crate::state::StorageKey`] from a hex string / unsigned integer
/// representation.
#[macro_export]
macro_rules! storage_key {
    ($s:expr) => {
        $crate::state::StorageKey(starknet_api::patricia_key!($s))
    };
}

/// A utility macro to create a [`crate::core::CompiledClassHash`] from a hex string /
/// unsigned integer representation.
#[macro_export]
macro_rules! compiled_class_hash {
    ($s:expr) => {
        $crate::core::CompiledClassHash(starknet_types_core::felt::Felt::from($s))
    };
}
