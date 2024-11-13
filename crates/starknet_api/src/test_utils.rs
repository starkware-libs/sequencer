use std::collections::HashMap;
use std::env;
use std::fs::read_to_string;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use starknet_types_core::felt::Felt;

use crate::core::{ContractAddress, Nonce};

pub mod declare;
pub mod deploy_account;
pub mod invoke;
pub mod l1_handler;

static PATH_TO_CARGO_MANIFEST_DIR: LazyLock<PathBuf> =
    LazyLock::new(|| Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).into());

/// Returns the absolute path from the project root.
pub fn get_absolute_path<P: AsRef<Path>>(relative_path: P) -> PathBuf {
    PATH_TO_CARGO_MANIFEST_DIR.join("../..").join(relative_path)
}

/// Returns the path to a file in the resources directory. This assumes the current working
/// directory has a `resources` folder. The value for file_path should be the path to the required
/// file in the folder "resources".
pub fn path_in_resources<P: AsRef<Path>>(file_path: P) -> PathBuf {
    PATH_TO_CARGO_MANIFEST_DIR.join("resources").join(file_path)
}

/// Reads from the directory containing the manifest at run time, same as current working directory.
pub fn read_json_file<P: AsRef<Path>>(path_in_resource_dir: P) -> serde_json::Value {
    let path = path_in_resources(path_in_resource_dir);
    let json_str = read_to_string(path.to_str().unwrap())
        .unwrap_or_else(|_| panic!("Failed to read file at path: {}", path.display()));
    serde_json::from_str(&json_str).unwrap()
}

#[derive(Debug, Default)]
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
