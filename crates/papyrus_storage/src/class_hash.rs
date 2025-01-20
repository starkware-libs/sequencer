//! Interface for handling hashes of Starknet [classes (Cairo 1)](https://docs.rs/starknet_api/latest/starknet_api/state/struct.ContractClass.html).
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

/// Interface for reading executable class hashes.
pub trait ClassHashStorageReader {
    /// Returns the executable class hash corresponding to the given class ID.
    /// Returns `None` if the class ID is not found.
    fn get_executable_class_hash(
        &self,
        class_hash: &ClassHash,
    ) -> StorageResult<Option<CompiledClassHash>>;
}

/// Interface for writing executable class hashes.
pub trait ClassHashStorageWriter
where
    Self: Sized,
{
    /// Inserts the executable class hash corresponding to the given class ID.
    fn set_executable_class_hash(
        self,
        class_hash: &ClassHash,
        executable_class_hash: CompiledClassHash,
    ) -> StorageResult<Self>;
}

impl<Mode: TransactionKind> ClassHashStorageReader for StorageTxn<'_, Mode> {
    fn get_executable_class_hash(
        &self,
        class_hash: &ClassHash,
    ) -> StorageResult<Option<CompiledClassHash>> {
        let table = self.open_table(&self.tables.class_hash_to_executable_class_hash)?;
        Ok(table.get(&self.txn, class_hash)?)
    }
}

impl ClassHashStorageWriter for StorageTxn<'_, RW> {
    fn set_executable_class_hash(
        self,
        class_hash: &ClassHash,
        executable_class_hash: CompiledClassHash,
    ) -> StorageResult<Self> {
        let table = self.open_table(&self.tables.class_hash_to_executable_class_hash)?;
        table.insert(&self.txn, class_hash, &executable_class_hash)?;
        Ok(self)
    }
}
