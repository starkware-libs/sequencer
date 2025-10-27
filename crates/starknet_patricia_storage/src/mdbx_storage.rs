use std::path::Path;

#[cfg(feature = "short_keys")]
use blake3::Hasher as Blake3Hasher;
use libmdbx::{
    Database as MdbxDb,
    DatabaseFlags,
    Geometry,
    PageSize,
    TableFlags,
    WriteFlags,
    WriteMap,
};
use page_size;
use tracing::info;

use crate::storage_trait::{DbHashMap, DbKey, DbValue, PatriciaStorageResult, Storage};

pub struct MdbxStorage {
    db: MdbxDb<WriteMap>,
    commit_counter: usize,
    stats_interval: Option<usize>,
    #[cfg(feature = "short_keys")]
    key_length: usize,
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

#[cfg(feature = "short_keys")]
fn get_short_key(key: &DbKey, key_length: usize) -> DbKey {
    let mut hasher = Blake3Hasher::new();
    hasher.update(&key.0);
    let mut short_key = vec![0; key_length];
    hasher.finalize_xof().fill(&mut short_key);
    DbKey(short_key)
}

impl MdbxStorage {
    pub fn open(path: &Path, stats_interval: Option<usize>, _key_length: Option<usize>) -> PatriciaStorageResult<Self> {
        // TODO(tzahi): geometry and related definitions are taken from apollo_storage. Check if
        // there are better configurations for the committer and consider moving boh crates mdbx
        // code to a common location.
        let db = MdbxDb::<WriteMap>::new()
            .set_geometry(Geometry {
                size: Some(1 << 20..1 << 40),
                growth_step: Some(1 << 32),
                page_size: Some(get_page_size(page_size::get())),
                ..Default::default()
            })
            .set_flags(DatabaseFlags {
                // As DbKeys are hashed, there is no locality of pages in the database almost at
                // all, so readahead will fill the RAM with garbage.
                // See https://libmdbx.dqdkfa.ru/group__c__opening.html#gga9138119a904355d245777c4119534061a16a07f878f8053cc79990063ca9510e7
                no_rdahead: true,
                // LIFO policy for recycling a Garbage Collection items should be faster when using
                // disks with write-back cache.
                liforeclaim: true,
                ..Default::default()
            })
            .open(path)?;
        let txn = db.begin_rw_txn()?;
        txn.create_table(None, TableFlags::empty())?;
        txn.commit()?;
        #[cfg(feature = "short_keys")]
        let storage = Self {
            db,
            commit_counter: 0,
            stats_interval,
            key_length: _key_length
                .expect("key_length must be provided when short_keys feature is enabled"),
        };
        #[cfg(not(feature = "short_keys"))]
        let storage = Self { db, commit_counter: 0, stats_interval };

        Ok(storage)
    }

    fn print_stats(&self) {
        match self.db.stat() {
            Ok(stat) => {
                info!(
                    "MDBX Database Statistics (after {} commits):\nPage size: {} bytes\nTree \
                     depth: {}\nBranch pages: {}\nLeaf pages: {}\nOverflow pages: {}\nEntries: {}",
                    self.commit_counter,
                    stat.page_size(),
                    stat.depth(),
                    stat.branch_pages(),
                    stat.leaf_pages(),
                    stat.overflow_pages(),
                    stat.entries()
                );
            }
            Err(e) => {
                info!("Failed to retrieve MDBX statistics: {}", e);
            }
        }
    }

    fn increment_and_check_stats(&mut self) {
        if let Some(interval) = self.stats_interval {
            self.commit_counter += 1;
            if self.commit_counter % interval == 0 {
                self.print_stats();
            }
        }
    }
}

impl Storage for MdbxStorage {
    fn get(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        let txn = self.db.begin_ro_txn()?;
        let table = txn.open_table(None)?;
        #[cfg(feature = "short_keys")]
        let key = get_short_key(key, self.key_length);
        Ok(txn.get(&table, &key.0)?.map(DbValue))
    }

    fn set(&mut self, key: DbKey, value: DbValue) -> PatriciaStorageResult<Option<DbValue>> {
        let txn = self.db.begin_rw_txn()?;
        let table = txn.open_table(None)?;
        #[cfg(feature = "short_keys")]
        let key = get_short_key(&key, self.key_length);
        let prev_val = txn.get(&table, &key.0)?.map(DbValue);
        txn.put(&table, key.0, value.0, WriteFlags::UPSERT)?;
        txn.commit()?;
        self.increment_and_check_stats();
        Ok(prev_val)
    }

    fn mget(&mut self, keys: &[&DbKey]) -> PatriciaStorageResult<Vec<Option<DbValue>>> {
        let txn = self.db.begin_ro_txn()?;
        let table = txn.open_table(None)?;
        let mut res = Vec::with_capacity(keys.len());
        for key in keys {
            #[cfg(feature = "short_keys")]
            let key = get_short_key(&key, self.key_length);
            res.push(txn.get(&table, &key.0)?.map(DbValue));
        }
        Ok(res)
    }

    fn mset(&mut self, key_to_value: DbHashMap) -> PatriciaStorageResult<()> {
        let txn = self.db.begin_rw_txn()?;
        let table = txn.open_table(None)?;
        for (key, value) in key_to_value {
            #[cfg(feature = "short_keys")]
            let key = get_short_key(&key, self.key_length);
            txn.put(&table, key.0, value.0, WriteFlags::UPSERT)?;
        }
        txn.commit()?;
        self.increment_and_check_stats();
        Ok(())
    }

    fn delete(&mut self, key: &DbKey) -> PatriciaStorageResult<Option<DbValue>> {
        let txn = self.db.begin_rw_txn()?;
        let table = txn.open_table(None)?;
        #[cfg(feature = "short_keys")]
        let key = get_short_key(key, self.key_length);
        let prev_val = txn.get(&table, &key.0)?.map(DbValue);
        if prev_val.is_some() {
            txn.del(&table, &key.0, None)?;
            txn.commit()?;
            self.increment_and_check_stats();
        }
        Ok(prev_val)
    }
}
