use std::fmt::Display;
use std::path::Path;
use std::sync::Arc;

use libmdbx::{
    Database as MdbxDb,
    DatabaseOptions,
    Mode,
    PageSize,
    ReadWriteOptions,
    Stat,
    TableFlags,
    WriteFlags,
    WriteMap,
};

use crate::storage_trait::{
    AsyncStorage,
    DbHashMap,
    DbKey,
    DbValue,
    EmptyStorageConfig,
    ImmutableReadOnlyStorage,
    PatriciaStorageResult,
    ReadOnlyStorage,
    Storage,
    StorageStats,
};

#[derive(Clone)]
pub struct MdbxStorage {
    db: Arc<MdbxDb<WriteMap>>,
}

// Size in bytes.
const MDBX_MIN_PAGESIZE: usize = 1 << 8; // 256 bytes
const MDBX_MAX_PAGESIZE: usize = 1 << 16; // 64KB

fn get_page_size(os_page_size: usize) -> PageSize {
    let mut page_size = os_page_size.clamp(MDBX_MIN_PAGESIZE, MDBX_MAX_PAGESIZE);

    // Page size must be power of two.
    if !page_size.is_power_of_two() {
        page_size = page_size.next_power_of_two() / 2;
    }

    PageSize::Set(page_size)
}

pub struct MdbxStorageStats(Stat);

impl StorageStats for MdbxStorageStats {
    fn column_titles() -> Vec<&'static str> {
        vec!["Page size", "Tree depth", "Branch pages", "Leaf pages", "Overflow pages"]
    }

    fn column_values(&self) -> Vec<String> {
        vec![
            self.0.page_size().to_string(),
            self.0.depth().to_string(),
            self.0.branch_pages().to_string(),
            self.0.leaf_pages().to_string(),
            self.0.overflow_pages().to_string(),
        ]
    }
}

impl Display for MdbxStorageStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MdbxStorageStats: {}", self.stat_string())
    }
}

impl MdbxStorage {
    pub fn open(path: &Path) -> PatriciaStorageResult<Self> {
        // TODO(tzahi): geometry and related definitions are taken from apollo_storage. Check if
        // there are better configurations for the committer and consider moving boh crates mdbx
        // code to a common location.
        let db = MdbxDb::<WriteMap>::open_with_options(
            path,
            DatabaseOptions {
                page_size: Some(get_page_size(page_size::get())),
                // As DbKeys are hashed, there is no locality of pages in the database almost at
                // all, so readahead will fill the RAM with garbage.
                // See https://libmdbx.dqdkfa.ru/group__c__opening.html#gga9138119a904355d245777c4119534061a16a07f878f8053cc79990063ca9510e7
                no_rdahead: true,
                // LIFO policy for recycling a Garbage Collection items should be faster when using
                // disks with write-back cache.
                liforeclaim: true,
                mode: Mode::ReadWrite(ReadWriteOptions {
                    min_size: Some(1 << 20),
                    max_size: Some(1 << 40),
                    growth_step: Some(1 << 32),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )?;
        let txn = db.begin_rw_txn()?;
        txn.create_table(None, TableFlags::empty())?;
        txn.commit()?;
        Ok(Self { db: Arc::new(db) })
    }
}

impl ImmutableReadOnlyStorage for MdbxStorage {
    async fn get(&self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        let txn = self.db.begin_ro_txn()?;
        let table = txn.open_table(None)?;
        Ok(txn.get(&table, &key.0)?.map(DbValue))
    }

    async fn mget(&self, keys: &[&DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        let txn = self.db.begin_ro_txn()?;
        let table = txn.open_table(None)?;
        let mut res = Vec::with_capacity(keys.len());
        for key in keys {
            res.push(txn.get(&table, &key.0)?.map(DbValue));
        }
        Ok(res)
    }
}

impl ReadOnlyStorage for MdbxStorage {
    async fn get_mut(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        ImmutableReadOnlyStorage::get(self, key).await
    }

    async fn mget_mut(&mut self, keys: &[&DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        ImmutableReadOnlyStorage::mget(self, keys).await
    }
}

impl Storage for MdbxStorage {
    type Stats = MdbxStorageStats;
    type Config = EmptyStorageConfig;

    async fn set(&mut self, key: DbKey, value: DbValue) -> PatriciaStorageResult<()> {
        let txn = self.db.begin_rw_txn()?;
        let table = txn.open_table(None)?;
        txn.put(&table, key.0, value.0, WriteFlags::UPSERT)?;
        txn.commit()?;
        Ok(())
    }

    async fn mset(&mut self, key_to_value: DbHashMap) -> PatriciaStorageResult<()> {
        let txn = self.db.begin_rw_txn()?;
        let table = txn.open_table(None)?;
        for (key, value) in key_to_value {
            txn.put(&table, key.0, value.0, WriteFlags::UPSERT)?;
        }
        txn.commit()?;
        Ok(())
    }

    async fn delete(&mut self, key: &DbKey) -> PatriciaStorageResult<()> {
        let txn = self.db.begin_rw_txn()?;
        let table = txn.open_table(None)?;
        txn.del(&table, &key.0, None)?;
        txn.commit()?;
        Ok(())
    }

    fn get_stats(&self) -> PatriciaStorageResult<Self::Stats> {
        Ok(MdbxStorageStats(self.db.stat()?))
    }

    fn get_async_self(&mut self) -> Option<&mut impl AsyncStorage> {
        Some(self)
    }
}

impl AsyncStorage for MdbxStorage {}
