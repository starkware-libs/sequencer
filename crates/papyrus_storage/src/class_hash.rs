//! Interface for handling hashes of Starknet [classes (Cairo 1)](https://docs.rs/starknet_api/latest/starknet_api/state/struct.ContractClass.html).
//! This is a table separate from Papyrus storage.
//!
//! Import [`ClassHashStorageReader`] and [`ClassHashStorageWriter`] to read and write data related
//! to classes using a [`StorageTxn`].

use std::marker::PhantomData;

use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::hash::StarkHash;

use crate::db::serialization::ValueSerde;
use crate::db::table_types::{SimpleTable, Table};
use crate::db::{DbError, DbReader, DbTransaction, DbWriter, TableHandle, TableIdentifier, RW};

#[cfg(test)]
#[path = "class_hash_test.rs"]
mod class_hash_test;

pub type ClassHashStorageResult<T> = Result<T, DbError>;

pub struct ClassHashStorage<'env> {
    table_id: TableIdentifier<ClassHash, CompiledClassHash, SimpleTable>,
    reader: DbReader,
    writer: DbWriter,
    _phantom: PhantomData<&'env ()>,
}

impl<'env> ClassHashStorage<'env> {
    pub fn new(
        reader: DbReader,
        mut writer: DbWriter,
        table_name: &'static str,
    ) -> ClassHashStorageResult<Self> {
        let table_id = writer.create_simple_table(table_name)?;
        Ok(Self { table_id, reader, writer, _phantom: PhantomData })
    }

    pub fn get_executable_class_hash(
        &self,
        class_hash: &ClassHash,
    ) -> ClassHashStorageResult<Option<CompiledClassHash>> {
        let txn = self.reader.begin_ro_txn()?;
        let table = txn.open_table(&self.table_id)?;
        table.get(&txn, class_hash)
    }

    pub fn set_executable_class_hash(
        &mut self,
        class_hash: &ClassHash,
        executable_class_hash: CompiledClassHash,
    ) -> ClassHashStorageResult<()> {
        let txn = self.writer.begin_rw_txn()?;
        let table = txn.open_table(&self.table_id)?;
        table.upsert(&txn, class_hash, &executable_class_hash)
    }
}

impl ValueSerde for CompiledClassHash {
    type Value = Self;

    fn serialize(value: &Self) -> Result<Vec<u8>, DbError> {
        Ok(value.0.to_bytes_be().to_vec())
    }

    fn deserialize(bytes: &mut impl std::io::Read) -> Option<Self::Value> {
        let mut hash_bytes = [0u8; 32];
        if bytes.read_exact(&mut hash_bytes).is_err() {
            return None;
        }

        Some(CompiledClassHash(StarkHash::from_bytes_be(&hash_bytes)))
    }
}
