use std::fmt::Display;

use rust_rocksdb::statistics::Ticker;
use rust_rocksdb::{properties as rprops, Cache, ColumnFamily, Options, DB};

use crate::rocksdb::{RocksDbStorage, HISTORICAL_TRIES_CF, LATEST_TRIE_CF};
use crate::storage_trait::StorageStats;

#[derive(Default, Debug, Clone)]
pub struct CfStats {
    pub l0_files: u64,
    pub level_stats: String,
    pub sst_size_mb: u64,
    pub pending_compaction_mb: u64,
    pub mem_active_mb: u64,
    pub immutable_memtables: u64,
    pub cache_usage_mb: u64,
}

#[derive(Default, Debug, Clone)]
pub struct RocksdbStorageStats {
    pub latest_stats: CfStats,
    pub history_stats: CfStats,
    pub block_cache_hit_rate_pct: f64,
}

fn get_int(db: &DB, prop: &str) -> u64 {
    db.property_int_value(prop).ok().flatten().unwrap_or(0)
}

fn collect_db_stats(db: &DB, cache: &Cache) -> CfStats {
    let to_mb = |bytes: u64| -> u64 { bytes / (1024 * 1024) };

    let l0_files = get_int(db, rprops::num_files_at_level(0).as_str());
    let sst_size_mb = to_mb(get_int(db, rprops::TOTAL_SST_FILES_SIZE.as_str()));
    let pending_compaction_mb =
        to_mb(get_int(db, rprops::ESTIMATE_PENDING_COMPACTION_BYTES.as_str()));
    let mem_active_mb = to_mb(get_int(db, rprops::CUR_SIZE_ACTIVE_MEM_TABLE.as_str()));
    let immutable_memtables = get_int(db, rprops::NUM_IMMUTABLE_MEM_TABLE.as_str());

    let cache_usage_mb = (cache.get_usage() as u64) / (1024 * 1024);
    let level_stats =
        db.property_value(rprops::LEVELSTATS.as_str()).ok().flatten().unwrap_or_default();

    CfStats {
        l0_files,
        level_stats,
        sst_size_mb,
        pending_compaction_mb,
        mem_active_mb,
        immutable_memtables,
        cache_usage_mb,
    }
}

impl RocksdbStorageStats {
    /// Collect stats for the provided column families.
    pub fn collect(db: &RocksDbStorage) -> Self {
        let latest_stats = collect_db_stats(&db.latest_db, &db.latest_cf_cache_handle);
        let history_stats = collect_db_stats(&db.history_db, &db.historical_cf_cache_handle);

        // Compute global cache hit rate if we have Options with statistics enabled.
        let hits = db.db_options.get_ticker_count(Ticker::BlockCacheHit);
        let misses = db.db_options.get_ticker_count(Ticker::BlockCacheMiss);
        let total = hits + misses;
        let block_cache_hit_rate_pct =
            if total > 0 { (hits as f64) * 100.0 / (total as f64) } else { 0.0 };

        Self { latest_stats, history_stats, block_cache_hit_rate_pct }
    }
}

impl Display for RocksdbStorageStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RocksdbStorageStats: {}", self.stat_string())
    }
}

impl StorageStats for RocksdbStorageStats {
    fn column_titles() -> Vec<&'static str> {
        let titles = vec![
            "latest||L0 files",
            "latest||level stats",
            "latest||SST size (MB)",
            "latest||Pending compaction (MB)",
            "latest||Active memtable (MB)",
            "latest||Immutable memtables",
            "latest||Cache usage (MB)",
            "historical||L0 files",
            "historical||level stats",
            "historical||SST size (MB)",
            "historical||Pending compaction (MB)",
            "historical||Active memtable (MB)",
            "historical||Immutable memtables",
            "historical||Cache usage (MB)",
            "global||Block cache hit rate (%)",
        ];

        titles
    }

    fn column_values(&self) -> Vec<String> {
        let mut values = Vec::new();

        extend_with_cf_stats(&mut values, &self.latest_stats);
        extend_with_cf_stats(&mut values, &self.history_stats);

        // Global metrics
        values.push(self.block_cache_hit_rate_pct.to_string());

        values
    }
}
fn extend_with_cf_stats(values: &mut Vec<String>, stats: &CfStats) {
    values.push(stats.l0_files.to_string());
    values.push(stats.level_stats.clone());
    values.push(stats.sst_size_mb.to_string());
    values.push(stats.pending_compaction_mb.to_string());
    values.push(stats.mem_active_mb.to_string());
    values.push(stats.immutable_memtables.to_string());
    values.push(stats.cache_usage_mb.to_string());
}
