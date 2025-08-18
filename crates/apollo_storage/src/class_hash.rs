//! Interface for handling hashes of Starknet [classes (Cairo 1)](https://docs.rs/starknet_api/latest/starknet_api/state/struct.ContractClass.html).
//! This is a table separate from Papyrus storage; scope and version do not apply on it.
//! Use carefully, only within class manager code, which is responsible for maintaining this table.
//!
//! Import [`ClassHashStorageReader`] and [`ClassHashStorageWriter`] to read and write data related
//! to classes using a [`StorageTxn`].

use starknet_api::core::{ClassHash, CompiledClassHash};

use crate::db::table_types::Table;
use crate::db::{TransactionKind, RW};
use crate::{StorageResult, StorageTxn};

#[cfg(test)]
#[path = "class_hash_test.rs"]
mod class_hash_test;

// TODO(Elin): consider implementing directly over `libmdbx`.

/// Interface for reading executable class hashes.
pub trait ClassHashStorageReader {
    /// Returns the executable class hash corresponding to the given class hash.
    /// Returns `None` if the class hash is not found.
    fn get_executable_class_hash_v2(
        &self,
        class_hash: &ClassHash,
    ) -> StorageResult<Option<CompiledClassHash>>;
}

/// Interface for writing executable class hashes.
pub trait ClassHashStorageWriter
where
    Self: Sized,
{
    /// Inserts the executable class hash corresponding to the given class hash.
    /// An error is returned if the class hash already exists.
    fn set_executable_class_hash_v2(
        self,
        class_hash: &ClassHash,
        executable_class_hash_v2: CompiledClassHash,
    ) -> StorageResult<Self>;
}

impl<Mode: TransactionKind> ClassHashStorageReader for StorageTxn<'_, Mode> {
    fn get_executable_class_hash_v2(
        &self,
        class_hash: &ClassHash,
    ) -> StorageResult<Option<CompiledClassHash>> {
        let table = self.open_table(&self.tables.stateless_compiled_class_hash_v2)?;
        Ok(table.get(&self.txn, class_hash)?)
    }
}

impl ClassHashStorageWriter for StorageTxn<'_, RW> {
    fn set_executable_class_hash_v2(
        self,
        class_hash: &ClassHash,
        executable_class_hash_v2: CompiledClassHash,
    ) -> StorageResult<Self> {
        let table = self.open_table(&self.tables.stateless_compiled_class_hash_v2)?;
        table.upsert(&self.txn, class_hash, &executable_class_hash_v2)?;
        Ok(self)
    }
}
