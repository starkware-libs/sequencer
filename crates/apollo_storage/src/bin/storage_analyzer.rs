#![cfg(feature = "storage_cli")]
#![allow(unused_assignments)] // Init-before-match patterns in scan functions.

//! Read-only analyzer for MDBX storage that projects flat state savings.
//!
//! Modes:
//!   (none)       — metadata only: per-table sizes from MDBX page stats (instant)
//!   --scan quick — count unique keys via next_nodup, estimate flat size (seconds)
//!   --scan deep  — full iteration: exact value sizes, per-block change counts,
//!                  key hotness distribution, changeset + history index projections (minutes)
//!
//! Usage:
//!   cargo run --release --bin storage_analyzer --features storage_cli -p apollo_storage -- \
//!       --db-path /data/batcher/SN_MAIN --db-path /data/sync/SN_MAIN --scan deep

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use clap::{Parser, ValueEnum};
use libmdbx::{DatabaseOptions, Mode, WriteMap, RO};

type Environment = libmdbx::Database<WriteMap>;
type RoTxn<'env> = libmdbx::Transaction<'env, RO, WriteMap>;

// ─── CLI ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, ValueEnum)]
enum ScanMode {
    /// Count unique keys via next_nodup (fast, O(unique_keys) for DupSort tables).
    Quick,
    /// Full iteration: measure exact value sizes, per-block change counts,
    /// key hotness, and project changeset + history index sizes.
    Deep,
}

#[derive(Parser, Debug)]
#[command(name = "storage_analyzer", about = "Analyze MDBX storage and project flat state savings")]
struct Args {
    /// Path(s) to MDBX database directories (containing mdbx.dat).
    /// Pass multiple to analyze both batcher and sync DBs.
    #[arg(long, required = true)]
    db_path: Vec<PathBuf>,

    /// Original storage directory for mmap file analysis (.dat files).
    /// Use when --db-path points to a temp copy of mdbx.dat but you want
    /// mmap stats from the original location. Defaults to --db-path.
    #[arg(long)]
    mmap_path: Option<Vec<PathBuf>>,

    /// Scan mode: "quick" (unique key counts) or "deep" (full entry iteration).
    /// Omit for metadata-only analysis.
    #[arg(long)]
    scan: Option<ScanMode>,

    /// Output format: "text" or "json".
    #[arg(long, default_value = "text")]
    format: String,
}

// ─── Table constants ─────────────────────────────────────────────────────────

/// DupSort versioned tables: main_key = state key, sub_key = BlockNumber.
const DUPSORT_VERSIONED_TABLES: &[&str] = &[
    "contract_storage",    // main: (ContractAddress, StorageKey), sub: BlockNumber
    "nonces",              // main: ContractAddress, sub: BlockNumber
    "compiled_class_hash", // main: ClassHash, sub: BlockNumber
];

/// SimpleTable versioned: key = (ContractAddress, BlockNumber).
const SIMPLE_VERSIONED_TABLES: &[&str] = &[
    "deployed_contracts",
];

const ALL_VERSIONED_TABLES: &[&str] = &[
    "contract_storage", "nonces", "deployed_contracts", "compiled_class_hash",
];

const RETAINED_TABLES: &[&str] = &[
    "block_hash_to_number", "block_signatures", "casms", "declared_classes",
    "declared_classes_block", "deprecated_declared_classes",
    "deprecated_declared_classes_block", "events", "headers", "last_voted_marker",
    "markers", "partial_block_hashes_components", "file_offsets", "state_diffs",
    "transaction_hash_to_idx", "transaction_metadata", "block_hashes", "global_root",
    "starknet_version", "storage_version", "stateless_compiled_class_hash_v2",
];

/// Mmap data files that live alongside mdbx.dat.
const MMAP_FILES: &[&str] = &[
    "thin_state_diff.dat",
    "contract_class.dat",
    "casm.dat",
    "deprecated_contract_class.dat",
    "transaction_output.dat",
    "transaction.dat",
];

const FLAT_TABLES: &[&str] = &[
    "flat_contract_storage", "flat_nonces", "flat_deployed_contracts", "flat_compiled_class_hash",
];
const CHANGESET_TABLES: &[&str] = &[
    "changeset_contract_storage", "changeset_nonces",
    "changeset_deployed_contracts", "changeset_compiled_class_hash",
];
const HISTORY_TABLES: &[&str] = &[
    "storage_history", "nonce_history", "deployed_contracts_history",
    "compiled_class_hash_history",
];

// ─── Data types ──────────────────────────────────────────────────────────────

struct TableInfo {
    entries: usize,
    total_size: u64,
}

/// Quick scan: unique key count + proportional estimates.
struct QuickScanResult {
    table_name: String,
    total_entries: usize,
    unique_keys: usize,
    avg_versions_per_key: f64,
    estimated_flat_bytes: u64,
}

/// Deep scan: exact measurements from full iteration.
struct DeepScanResult {
    table_name: String,
    total_entries: usize,
    unique_keys: usize,
    avg_versions_per_key: f64,
    /// Sum of the latest value byte length for each unique key.
    exact_flat_value_bytes: u64,
    /// Sum of the main key byte length for each unique key.
    exact_flat_key_bytes: u64,
    /// Number of distinct block numbers seen (= blocks that touched this table).
    distinct_blocks: usize,
    /// Average number of keys changed per block.
    avg_keys_per_block: f64,
    /// Max number of versions for any single key (hottest key).
    max_versions_single_key: usize,
    /// Top-10 hottest keys by version count (version_count only, not the key itself).
    top_hot_key_counts: Vec<usize>,
    /// Per-block entry counts (block_number -> entries_changed). Full map kept
    /// so we can project changeset sizes for any retention window.
    block_entry_counts: BTreeMap<u64, usize>,
    /// Total value bytes across all entries (for avg value size).
    total_value_bytes: u64,
}

struct MmapFileInfo {
    name: String,
    size: u64,
}

struct DbAnalysis {
    label: String,
    table_stats: BTreeMap<String, TableInfo>,
    total_db_size: u64,
    page_size: u64,
    versioned_size: u64,
    retained_size: u64,
    flat_size: u64,
    changeset_size: u64,
    history_size: u64,
    quick_scans: Vec<QuickScanResult>,
    deep_scans: Vec<DeepScanResult>,
    /// Block height read from markers table (State marker).
    block_height: Option<u64>,
    /// Mmap data files alongside mdbx.dat.
    mmap_files: Vec<MmapFileInfo>,
    /// Total mmap file size.
    mmap_total_size: u64,
}

// ─── Helpers: markers, mmap ──────────────────────────────────────────────────

/// Read block height from the markers table.
/// MarkerKind::State = 3u8, value = 0x00 (version byte) + u32 BE (block number).
fn read_block_height(txn: &RoTxn<'_>) -> Option<u64> {
    let table = txn.open_table(Some("markers")).ok()?;
    let mut cursor = txn.cursor(&table).ok()?;
    // Iterate all marker entries and find MarkerKind::State (key byte = 3).
    let mut entry: Option<(Cow<'_, [u8]>, Cow<'_, [u8]>)> = cursor.first().ok()?;
    loop {
        match entry {
            None => return None,
            Some((key, val)) => {
                if key.as_ref() == &[3u8] {
                    // Value: version byte (0x00) + u32 BE block number.
                    if val.len() >= 5 {
                        return Some(
                            u32::from_be_bytes(val[1..5].try_into().ok()?) as u64,
                        );
                    } else if val.len() >= 4 {
                        return Some(
                            u32::from_be_bytes(val[..4].try_into().ok()?) as u64,
                        );
                    }
                    return None;
                }
                entry = cursor.next().ok()?;
            }
        }
    }
}

/// Stat mmap data files in the same directory as mdbx.dat.
fn read_mmap_file_sizes(db_path: &PathBuf) -> Vec<MmapFileInfo> {
    MMAP_FILES
        .iter()
        .filter_map(|name| {
            let path = db_path.join(name);
            let meta = fs::metadata(&path).ok()?;
            Some(MmapFileInfo { name: name.to_string(), size: meta.len() })
        })
        .collect()
}

// ─── Utilities ───────────────────────────────────────────────────────────────

fn human_bytes(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.2} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.2} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1_024 {
        format!("{:.2} KB", bytes as f64 / 1_024.0)
    } else {
        format!("{bytes} B")
    }
}

fn sum_sizes(stats: &BTreeMap<String, TableInfo>, names: &[&str]) -> u64 {
    names.iter().filter_map(|n| stats.get(*n)).map(|s| s.total_size).sum()
}

fn pct(part: u64, total: u64) -> f64 {
    if total == 0 { 0.0 } else { part as f64 / total as f64 * 100.0 }
}

// ─── Quick scan ──────────────────────────────────────────────────────────────

fn quick_scan_dupsort(
    txn: &RoTxn<'_>,
    table_name: &str,
    info: &TableInfo,
) -> Option<QuickScanResult> {
    let table = txn.open_table(Some(table_name)).ok()?;
    let mut cursor = txn.cursor(&table).ok()?;
    let start = Instant::now();

    let mut unique_keys: usize = 0;
    let first: Option<(Cow<'_, [u8]>, Cow<'_, [u8]>)> = cursor.first().ok()?;
    if first.is_none() {
        return Some(make_quick_result(table_name, info, 0));
    }
    unique_keys += 1;
    loop {
        match cursor.next_nodup::<Cow<'_, [u8]>, Cow<'_, [u8]>>() {
            Ok(Some(_)) => {
                unique_keys += 1;
                if unique_keys % 1_000_000 == 0 {
                    eprint!("\r  {table_name}: {}M unique keys so far ({:.0}s)...",
                        unique_keys / 1_000_000, start.elapsed().as_secs_f64());
                    let _ = std::io::stderr().flush();
                }
            }
            _ => break,
        }
    }
    if unique_keys >= 1_000_000 {
        eprint!("\r{:80}\r", "");
        let _ = std::io::stderr().flush();
    }
    Some(make_quick_result(table_name, info, unique_keys))
}

fn quick_scan_simple(
    txn: &RoTxn<'_>,
    table_name: &str,
    info: &TableInfo,
    prefix_len: usize,
) -> Option<QuickScanResult> {
    let table = txn.open_table(Some(table_name)).ok()?;
    let mut cursor = txn.cursor(&table).ok()?;
    let start = Instant::now();

    let mut unique_keys: usize = 0;
    let mut scanned: usize = 0;
    let mut last_prefix: Option<Vec<u8>> = None;

    let first: Option<(Cow<'_, [u8]>, Cow<'_, [u8]>)> = cursor.first().ok()?;
    match first {
        None => return Some(make_quick_result(table_name, info, 0)),
        Some((key, _)) if key.len() >= prefix_len => {
            last_prefix = Some(key[..prefix_len].to_vec());
            unique_keys = 1;
            scanned = 1;
        }
        _ => return Some(make_quick_result(table_name, info, 0)),
    }

    loop {
        match cursor.next::<Cow<'_, [u8]>, Cow<'_, [u8]>>() {
            Ok(Some((key, _))) if key.len() >= prefix_len => {
                scanned += 1;
                let prefix = &key[..prefix_len];
                if last_prefix.as_deref() != Some(prefix) {
                    unique_keys += 1;
                    last_prefix = Some(prefix.to_vec());
                }
                if scanned % 1_000_000 == 0 {
                    let pct_done = scanned as f64 / info.entries as f64 * 100.0;
                    eprint!("\r  {table_name}: {:.0}% ({}/{}), {:.0}s...",
                        pct_done, scanned, info.entries, start.elapsed().as_secs_f64());
                    let _ = std::io::stderr().flush();
                }
            }
            _ => break,
        }
    }
    if scanned >= 1_000_000 {
        eprint!("\r{:80}\r", "");
        let _ = std::io::stderr().flush();
    }
    Some(make_quick_result(table_name, info, unique_keys))
}

fn make_quick_result(table_name: &str, info: &TableInfo, unique_keys: usize) -> QuickScanResult {
    let total = info.entries;
    let avg_ver = if unique_keys > 0 { total as f64 / unique_keys as f64 } else { 0.0 };
    let est_flat =
        if total > 0 { (info.total_size as f64 * unique_keys as f64 / total as f64) as u64 } else { 0 };
    QuickScanResult {
        table_name: table_name.to_string(),
        total_entries: total,
        unique_keys,
        avg_versions_per_key: avg_ver,
        estimated_flat_bytes: est_flat,
    }
}

// ─── Deep scan ───────────────────────────────────────────────────────────────

/// Deep scan a DupSort table: iterates every entry, collecting exact sizes,
/// per-block counts, key hotness, and the latest value per unique key.
fn deep_scan_dupsort(
    txn: &RoTxn<'_>,
    table_name: &str,
    info: &TableInfo,
) -> Option<DeepScanResult> {
    let table = txn.open_table(Some(table_name)).ok()?;
    let mut cursor = txn.cursor(&table).ok()?;
    let start = Instant::now();

    let mut unique_keys: usize = 0;
    let mut exact_flat_value_bytes: u64 = 0;
    let mut exact_flat_key_bytes: u64 = 0;
    let mut total_value_bytes: u64 = 0;
    let mut total_entries: usize = 0;
    let mut max_versions_single_key: usize = 0;
    let mut current_key_versions: usize = 0;
    let mut hot_keys: Vec<usize> = Vec::new(); // track top-N version counts
    let mut block_counts: BTreeMap<u64, usize> = BTreeMap::new();

    // DupSort: iterate with next(). Main key changes are detected by next_nodup logic,
    // but to get values we need to iterate every entry.
    // Strategy: use first() then next(), track when main key changes.
    let first: Option<(Cow<'_, [u8]>, Cow<'_, [u8]>)> = cursor.first().ok()?;
    let (mut prev_main_key, first_val) = match first {
        None => return Some(make_deep_result_empty(table_name, info)),
        Some((k, v)) => (k.to_vec(), v),
    };

    unique_keys = 1;
    current_key_versions = 1;
    total_entries = 1;
    total_value_bytes += first_val.len() as u64;
    // For DupSort, the sub-key (BlockNumber) is in the value's first 8 bytes,
    // or it could be the dup data. Actually in MDBX CommonPrefix, the sub-key
    // is stored as part of the dup data. The "value" from cursor includes the sub-key.
    // For block counting, we need to extract the block number from the sub-key portion.
    // In CommonPrefix tables, the cursor key is the main key and the value starts with
    // the sub-key bytes. BlockNumber is serialized as big-endian u64.
    let mut latest_value = first_val.to_vec();
    let mut latest_key = prev_main_key.clone();
    // DupSort value layout: sub_key (BlockNumber = 4 bytes u32 BE) + version_byte + value.
    if first_val.len() >= 4 {
        let block = u32::from_be_bytes(first_val[..4].try_into().unwrap_or([0; 4])) as u64;
        *block_counts.entry(block).or_insert(0) += 1;
    }

    loop {
        match cursor.next::<Cow<'_, [u8]>, Cow<'_, [u8]>>() {
            Ok(Some((key, val))) => {
                total_entries += 1;
                total_value_bytes += val.len() as u64;

                if total_entries % 5_000_000 == 0 {
                    let pct_done = total_entries as f64 / info.entries as f64 * 100.0;
                    eprint!("\r  {table_name}: {:.0}% ({}/{}), {} unique, {:.0}s...",
                        pct_done, total_entries, info.entries, unique_keys,
                        start.elapsed().as_secs_f64());
                    let _ = std::io::stderr().flush();
                }

                if val.len() >= 4 {
                    let block = u32::from_be_bytes(val[..4].try_into().unwrap_or([0; 4])) as u64;
                    *block_counts.entry(block).or_insert(0) += 1;
                }

                if key.as_ref() != prev_main_key.as_slice() {
                    // New main key — finalize previous.
                    exact_flat_key_bytes += latest_key.len() as u64;
                    // Latest value = value with highest block number.
                    // In sorted DupSort, the last value before key change is the latest.
                    exact_flat_value_bytes += latest_value.len() as u64;
                    if current_key_versions > max_versions_single_key {
                        max_versions_single_key = current_key_versions;
                    }
                    hot_keys.push(current_key_versions);

                    unique_keys += 1;
                    current_key_versions = 1;
                    prev_main_key = key.to_vec();
                    latest_key = key.to_vec();
                    latest_value = val.to_vec();
                } else {
                    current_key_versions += 1;
                    // In sorted order, later entries have higher block numbers = more recent.
                    latest_value = val.to_vec();
                }
            }
            _ => break,
        }
    }

    // Finalize last key.
    if unique_keys > 0 {
        exact_flat_key_bytes += latest_key.len() as u64;
        exact_flat_value_bytes += latest_value.len() as u64;
        if current_key_versions > max_versions_single_key {
            max_versions_single_key = current_key_versions;
        }
        hot_keys.push(current_key_versions);
    }

    // Top-10 hottest keys.
    hot_keys.sort_unstable_by(|a, b| b.cmp(a));
    hot_keys.truncate(10);

    let distinct_blocks = block_counts.len();
    let avg_keys_per_block = if distinct_blocks > 0 {
        total_entries as f64 / distinct_blocks as f64
    } else {
        0.0
    };

    if total_entries >= 5_000_000 {
        eprint!("\r{:80}\r", "");  // Clear progress line.
    }

    Some(DeepScanResult {
        table_name: table_name.to_string(),
        total_entries,
        unique_keys,
        avg_versions_per_key: if unique_keys > 0 {
            total_entries as f64 / unique_keys as f64
        } else {
            0.0
        },
        exact_flat_value_bytes,
        exact_flat_key_bytes,
        distinct_blocks,
        avg_keys_per_block,
        max_versions_single_key,
        top_hot_key_counts: hot_keys,
        block_entry_counts: block_counts,
        total_value_bytes,
    })
}

/// Deep scan a SimpleTable: iterates every entry.
fn deep_scan_simple(
    txn: &RoTxn<'_>,
    table_name: &str,
    info: &TableInfo,
    prefix_len: usize,
) -> Option<DeepScanResult> {
    let table = txn.open_table(Some(table_name)).ok()?;
    let mut cursor = txn.cursor(&table).ok()?;
    let start = Instant::now();

    let mut unique_keys: usize = 0;
    let mut exact_flat_value_bytes: u64 = 0;
    let mut exact_flat_key_bytes: u64 = 0;
    let mut total_value_bytes: u64 = 0;
    let mut total_entries: usize = 0;
    let mut max_versions_single_key: usize = 0;
    let mut current_key_versions: usize = 0;
    let mut hot_keys: Vec<usize> = Vec::new();
    let mut block_counts: BTreeMap<u64, usize> = BTreeMap::new();
    let mut last_prefix: Option<Vec<u8>> = None;
    let mut latest_value: Vec<u8> = Vec::new();

    let first: Option<(Cow<'_, [u8]>, Cow<'_, [u8]>)> = cursor.first().ok()?;
    match first {
        None => return Some(make_deep_result_empty(table_name, info)),
        Some((key, val)) => {
            total_entries = 1;
            total_value_bytes += val.len() as u64;
            latest_value = val.to_vec();

            if key.len() >= prefix_len {
                last_prefix = Some(key[..prefix_len].to_vec());
                unique_keys = 1;
                current_key_versions = 1;
            }
            // Extract block number from key suffix.
            if key.len() > prefix_len {
                let block_bytes = &key[prefix_len..];
                if block_bytes.len() >= 4 {
                    let block = u32::from_be_bytes(block_bytes[..4].try_into().unwrap_or([0; 4])) as u64;
                    *block_counts.entry(block).or_insert(0) += 1;
                }
            }
        }
    }

    loop {
        match cursor.next::<Cow<'_, [u8]>, Cow<'_, [u8]>>() {
            Ok(Some((key, val))) => {
                total_entries += 1;
                total_value_bytes += val.len() as u64;

                if total_entries % 1_000_000 == 0 {
                    let pct_done = total_entries as f64 / info.entries as f64 * 100.0;
                    eprint!("\r  {table_name}: {:.0}% ({}/{}), {} unique, {:.0}s...",
                        pct_done, total_entries, info.entries, unique_keys,
                        start.elapsed().as_secs_f64());
                    let _ = std::io::stderr().flush();
                }

                if key.len() > prefix_len {
                    let block_bytes = &key[prefix_len..];
                    if block_bytes.len() >= 8 {
                        let block =
                            u64::from_be_bytes(block_bytes[..8].try_into().unwrap_or([0; 8]));
                        *block_counts.entry(block).or_insert(0) += 1;
                    }
                }

                if key.len() >= prefix_len {
                    let prefix = &key[..prefix_len];
                    if last_prefix.as_deref() != Some(prefix) {
                        // New key — finalize previous.
                        if let Some(prev_prefix) = &last_prefix {
                            exact_flat_key_bytes += prev_prefix.len() as u64;
                            exact_flat_value_bytes += latest_value.len() as u64;
                            if current_key_versions > max_versions_single_key {
                                max_versions_single_key = current_key_versions;
                            }
                            hot_keys.push(current_key_versions);
                        }
                        unique_keys += 1;
                        current_key_versions = 1;
                        last_prefix = Some(prefix.to_vec());
                        latest_value = val.to_vec();
                    } else {
                        current_key_versions += 1;
                        latest_value = val.to_vec();
                    }
                }
            }
            _ => break,
        }
    }

    // Finalize last key.
    if let Some(prev_prefix) = &last_prefix {
        exact_flat_key_bytes += prev_prefix.len() as u64;
        exact_flat_value_bytes += latest_value.len() as u64;
        if current_key_versions > max_versions_single_key {
            max_versions_single_key = current_key_versions;
        }
        hot_keys.push(current_key_versions);
    }

    hot_keys.sort_unstable_by(|a, b| b.cmp(a));
    hot_keys.truncate(10);

    let distinct_blocks = block_counts.len();
    let avg_keys_per_block =
        if distinct_blocks > 0 { total_entries as f64 / distinct_blocks as f64 } else { 0.0 };

    if total_entries >= 1_000_000 {
        eprint!("\r{:80}\r", "");
    }

    Some(DeepScanResult {
        table_name: table_name.to_string(),
        total_entries,
        unique_keys,
        avg_versions_per_key: if unique_keys > 0 {
            total_entries as f64 / unique_keys as f64
        } else {
            0.0
        },
        exact_flat_value_bytes,
        exact_flat_key_bytes,
        distinct_blocks,
        avg_keys_per_block,
        max_versions_single_key,
        top_hot_key_counts: hot_keys,
        block_entry_counts: block_counts,
        total_value_bytes,
    })
}

fn make_deep_result_empty(table_name: &str, _info: &TableInfo) -> DeepScanResult {
    DeepScanResult {
        table_name: table_name.to_string(),
        total_entries: 0,
        unique_keys: 0,
        avg_versions_per_key: 0.0,

        exact_flat_value_bytes: 0,
        exact_flat_key_bytes: 0,
        distinct_blocks: 0,
        avg_keys_per_block: 0.0,
        max_versions_single_key: 0,
        top_hot_key_counts: Vec::new(),
        block_entry_counts: BTreeMap::new(),
        total_value_bytes: 0,
    }
}

// ─── DB analysis ─────────────────────────────────────────────────────────────

fn detect_prefix_len(txn: &RoTxn<'_>, table_name: &str) -> Option<usize> {
    let table = txn.open_table(Some(table_name)).ok()?;
    let mut cursor = txn.cursor(&table).ok()?;
    let first: Option<(Cow<'_, [u8]>, Cow<'_, [u8]>)> = cursor.first().ok()?;
    let (key, _) = first?;
    // BlockNumber is serialized as u32 big-endian (4 bytes) at the end of the key.
    if key.len() > 4 { Some(key.len() - 4) } else { Some(key.len()) }
}

fn analyze_db(
    db_path: &PathBuf,
    mmap_path: &PathBuf,
    scan: &Option<ScanMode>,
) -> DbAnalysis {
    let db_file = db_path.join("mdbx.dat");
    if !db_file.exists() {
        eprintln!("Error: {} does not exist", db_file.display());
        eprintln!("--db-path should point to the directory containing mdbx.dat");
        std::process::exit(1);
    }

    let env = match Environment::open_with_options(
        db_path,
        DatabaseOptions {
            max_tables: Some(37),
            no_rdahead: true,
            exclusive: false,
            mode: Mode::ReadOnly,
            ..Default::default()
        },
    ) {
        Ok(env) => env,
        Err(err) => {
            eprintln!("Error opening database at {}: {err}", db_path.display());
            std::process::exit(1);
        }
    };

    let env_stat = env.stat().expect("Failed to read env stats");
    let page_size = env_stat.page_size() as u64;
    let total_db_size = env_stat.total_size();

    let txn = env.begin_ro_txn().expect("Failed to begin read transaction");
    let mut table_stats: BTreeMap<String, TableInfo> = BTreeMap::new();

    let all_table_names: Vec<&str> = ALL_VERSIONED_TABLES
        .iter()
        .chain(RETAINED_TABLES.iter())
        .chain(FLAT_TABLES.iter())
        .chain(CHANGESET_TABLES.iter())
        .chain(HISTORY_TABLES.iter())
        .copied()
        .collect();

    for name in &all_table_names {
        if let Ok(table) = txn.open_table(Some(name)) {
            if let Ok(stat) = txn.table_stat(&table) {
                table_stats.insert(
                    name.to_string(),
                    TableInfo { entries: stat.entries(), total_size: stat.total_size() },
                );
            }
        }
    }

    let mut quick_scans = Vec::new();
    let mut deep_scans = Vec::new();

    match scan {
        Some(ScanMode::Quick) => {
            eprintln!("Quick scan: {}...", db_path.display());
            for name in DUPSORT_VERSIONED_TABLES {
                if let Some(info) = table_stats.get(*name) {
                    eprint!("  {name} ({} entries)... ", info.entries);
                    if let Some(r) = quick_scan_dupsort(&txn, name, info) {
                        eprintln!("{} unique keys, {:.1}x avg", r.unique_keys, r.avg_versions_per_key);
                        quick_scans.push(r);
                    } else {
                        eprintln!("skipped");
                    }
                }
            }
            for name in SIMPLE_VERSIONED_TABLES {
                if let Some(info) = table_stats.get(*name) {
                    eprint!("  {name} ({} entries)... ", info.entries);
                    let prefix_len = detect_prefix_len(&txn, name).unwrap_or(32);
                    if let Some(r) = quick_scan_simple(&txn, name, info, prefix_len) {
                        eprintln!("{} unique keys, {:.1}x avg", r.unique_keys, r.avg_versions_per_key);
                        quick_scans.push(r);
                    } else {
                        eprintln!("skipped");
                    }
                }
            }
        }
        Some(ScanMode::Deep) => {
            eprintln!("Deep scan: {} (this may take minutes)...", db_path.display());
            for name in DUPSORT_VERSIONED_TABLES {
                if let Some(info) = table_stats.get(*name) {
                    eprint!("  {name} ({} entries)... ", info.entries);
                    if let Some(r) = deep_scan_dupsort(&txn, name, info) {
                        eprintln!(
                            "{} unique keys, {:.1}x avg, {} blocks, hottest key: {} versions",
                            r.unique_keys, r.avg_versions_per_key, r.distinct_blocks,
                            r.max_versions_single_key,
                        );
                        deep_scans.push(r);
                    } else {
                        eprintln!("skipped");
                    }
                }
            }
            for name in SIMPLE_VERSIONED_TABLES {
                if let Some(info) = table_stats.get(*name) {
                    eprint!("  {name} ({} entries)... ", info.entries);
                    let prefix_len = detect_prefix_len(&txn, name).unwrap_or(32);
                    if let Some(r) =
                        deep_scan_simple(&txn, name, info, prefix_len)
                    {
                        eprintln!(
                            "{} unique keys, {:.1}x avg, {} blocks",
                            r.unique_keys, r.avg_versions_per_key, r.distinct_blocks,
                        );
                        deep_scans.push(r);
                    } else {
                        eprintln!("skipped");
                    }
                }
            }
        }
        None => {}
    }

    // Read block height from markers table.
    let block_height = read_block_height(&txn);

    // Read mmap file sizes from the mmap path (may differ from db_path).
    let mmap_files = read_mmap_file_sizes(mmap_path);
    let mmap_total_size: u64 = mmap_files.iter().map(|f| f.size).sum();

    let label = db_path.display().to_string();
    DbAnalysis {
        label,
        versioned_size: sum_sizes(&table_stats, ALL_VERSIONED_TABLES),
        retained_size: sum_sizes(&table_stats, RETAINED_TABLES),
        flat_size: sum_sizes(&table_stats, FLAT_TABLES),
        changeset_size: sum_sizes(&table_stats, CHANGESET_TABLES),
        history_size: sum_sizes(&table_stats, HISTORY_TABLES),
        table_stats,
        total_db_size,
        page_size,
        quick_scans,
        deep_scans,
        block_height,
        mmap_files,
        mmap_total_size,
    }
}

// ─── main ────────────────────────────────────────────────────────────────────

fn main() {
    let args = Args::parse();
    let mmap_paths = args.mmap_path.as_ref().unwrap_or(&args.db_path);
    let analyses: Vec<DbAnalysis> = args
        .db_path
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let mmap_p = mmap_paths.get(i).unwrap_or(p);
            analyze_db(p, mmap_p, &args.scan)
        })
        .collect();

    if args.format == "json" {
        println!("[");
        for (i, a) in analyses.iter().enumerate() {
            print_json_entry(a);
            if i < analyses.len() - 1 { println!(","); } else { println!(); }
        }
        println!("]");
    } else {
        for a in &analyses {
            print_text(a);
        }
        if analyses.len() > 1 {
            print_combined_summary(&analyses);
        }
    }
}

// ─── Output: shared ──────────────────────────────────────────────────────────

fn print_table_group(stats: &BTreeMap<String, TableInfo>, total: u64, names: &[&str]) {
    for name in names {
        if let Some(s) = stats.get(*name) {
            if s.total_size > 0 {
                println!(
                    "    {:<40} {:>12} {:>14} {:>5.1}%",
                    name, human_bytes(s.total_size), s.entries, pct(s.total_size, total)
                );
            }
        }
    }
}

fn print_text(a: &DbAnalysis) {
    println!();
    println!("╔══════════════════════════════════════════════════════════════════════╗");
    println!("║                   MDBX Storage Analysis Report                      ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("  Database: {}", a.label);
    println!("    MDBX size:   {}    Page size:  {}", human_bytes(a.total_db_size), human_bytes(a.page_size));
    if let Some(height) = a.block_height {
        println!("    Block height: {height}");
    }
    if a.mmap_total_size > 0 {
        println!("    Mmap files:  {}", human_bytes(a.mmap_total_size));
    }
    let total_disk = a.total_db_size + a.mmap_total_size;
    if a.mmap_total_size > 0 {
        println!("    Total disk:  {} (MDBX + mmap)", human_bytes(total_disk));
    }
    println!();

    // Table breakdown.
    println!("  VERSIONED STATE (to be replaced)");
    print_table_group(&a.table_stats, a.total_db_size, ALL_VERSIONED_TABLES);
    println!("    {:<40} {:>12} {:>14} {:>5.1}%",
        "SUBTOTAL", human_bytes(a.versioned_size), "", pct(a.versioned_size, a.total_db_size));
    println!();

    println!("  RETAINED (unaffected)");
    print_table_group(&a.table_stats, a.total_db_size, RETAINED_TABLES);
    println!("    {:<40} {:>12} {:>14} {:>5.1}%",
        "SUBTOTAL", human_bytes(a.retained_size), "", pct(a.retained_size, a.total_db_size));

    println!();

    // Mmap files.
    if !a.mmap_files.is_empty() {
        println!("  MMAP DATA FILES (outside MDBX, not affected by flat state)");
        for f in &a.mmap_files {
            if f.size > 0 {
                println!("    {:<40} {:>12}", f.name, human_bytes(f.size));
            }
        }
        println!("    {:<40} {:>12}", "SUBTOTAL", human_bytes(a.mmap_total_size));
        println!();
    }

    // Growth rate estimate from block height.
    if let Some(height) = a.block_height {
        if height > 0 {
            let mdbx_per_block = a.total_db_size as f64 / height as f64;
            let versioned_per_block = a.versioned_size as f64 / height as f64;
            println!("  GROWTH RATE (avg over {} blocks)", height);
            println!("    MDBX per block:      {:.0} bytes ({:.2} KB)", mdbx_per_block, mdbx_per_block / 1024.0);
            println!("    Versioned per block:  {:.0} bytes ({:.2} KB)", versioned_per_block, versioned_per_block / 1024.0);
            if a.mmap_total_size > 0 {
                let total_per_block = (a.total_db_size + a.mmap_total_size) as f64 / height as f64;
                println!("    Total per block:     {:.0} bytes ({:.2} KB)", total_per_block, total_per_block / 1024.0);
            }
            println!();
        }
    }

    if a.flat_size > 0 || a.changeset_size > 0 || a.history_size > 0 {
        println!("  FLAT STATE (new tables, populated)");
        print_table_group(&a.table_stats, a.total_db_size, FLAT_TABLES);
        print_table_group(&a.table_stats, a.total_db_size, CHANGESET_TABLES);
        print_table_group(&a.table_stats, a.total_db_size, HISTORY_TABLES);
        println!();
    }

    // Quick scan results.
    if !a.quick_scans.is_empty() {
        print_quick_scan_results(a);
    }

    // Deep scan results.
    if !a.deep_scans.is_empty() {
        print_deep_scan_results(a);
    }

    // No scan.
    if a.quick_scans.is_empty() && a.deep_scans.is_empty() {
        println!("  Use --scan quick or --scan deep for projections.");
        println!("  Versioned: {} ({:.1}%)  Retained: {} ({:.1}%)",
            human_bytes(a.versioned_size), pct(a.versioned_size, a.total_db_size),
            human_bytes(a.retained_size), pct(a.retained_size, a.total_db_size));
    }
}

fn print_quick_scan_results(a: &DbAnalysis) {
    println!("  ── QUICK SCAN ────────────────────────────────────────────────────");
    println!();
    println!("    {:<25} {:>12} {:>12} {:>8} {:>12}", "Table", "Entries", "Unique", "Avg Ver", "Est. Flat");
    println!("    {:<25} {:>12} {:>12} {:>8} {:>12}", "─────", "───────", "──────", "───────", "─────────");

    let mut total_flat: u64 = 0;
    let mut total_unique: usize = 0;
    let mut total_entries: usize = 0;
    for r in &a.quick_scans {
        println!("    {:<25} {:>12} {:>12} {:>7.1}x {:>12}",
            r.table_name, r.total_entries, r.unique_keys,
            r.avg_versions_per_key, human_bytes(r.estimated_flat_bytes));
        total_flat += r.estimated_flat_bytes;
        total_unique += r.unique_keys;
        total_entries += r.total_entries;
    }
    println!();
    println!("    Unique keys: {total_unique}    Entries: {total_entries}    Avg versions/key: {:.1}x",
        if total_unique > 0 { total_entries as f64 / total_unique as f64 } else { 0.0 });
    println!();

    let projected = a.retained_size + total_flat;
    println!("    Projected (retained + est flat):  {}", human_bytes(projected));
    println!("    Current:                          {}", human_bytes(a.total_db_size));
    if a.total_db_size > projected {
        println!("    Max savings:                      {} ({:.1}%)",
            human_bytes(a.total_db_size - projected), pct(a.total_db_size - projected, a.total_db_size));
    }
    println!("    (excludes changesets + history — use --scan deep for those)");
    println!();
}

/// Compute changeset bytes for a given retention window across all deep scan results.
fn changeset_bytes_for_window(deep_scans: &[DeepScanResult], window: u64) -> (usize, u64) {
    let mut total_entries: usize = 0;
    let mut total_bytes: u64 = 0;
    for r in deep_scans {
        if r.block_entry_counts.is_empty() {
            continue;
        }
        let max_block = *r.block_entry_counts.keys().next_back().unwrap();
        let min_block = max_block.saturating_sub(window);
        let tail_entries: usize = r
            .block_entry_counts
            .range(min_block..)
            .map(|(_, c)| c)
            .sum();
        let avg_val = if r.total_entries > 0 {
            r.total_value_bytes as f64 / r.total_entries as f64
        } else {
            0.0
        };
        total_entries += tail_entries;
        total_bytes += (tail_entries as f64 * avg_val) as u64;
    }
    (total_entries, total_bytes)
}

fn print_deep_scan_results(a: &DbAnalysis) {
    println!("  ── DEEP SCAN ─────────────────────────────────────────────────────");
    println!();

    // Per-table summary.
    println!("    {:<25} {:>10} {:>10} {:>7} {:>8} {:>10} {:>10}",
        "Table", "Entries", "Unique", "Avg", "Blocks", "Flat Keys", "Flat Vals");
    println!("    {:<25} {:>10} {:>10} {:>7} {:>8} {:>10} {:>10}",
        "─────", "───────", "──────", "───", "──────", "─────────", "─────────");

    let mut total_flat_keys: u64 = 0;
    let mut total_flat_vals: u64 = 0;
    let mut total_unique: usize = 0;
    let mut total_entries: usize = 0;
    let mut total_distinct_blocks: usize = 0;

    for r in &a.deep_scans {
        println!("    {:<25} {:>10} {:>10} {:>6.1}x {:>8} {:>10} {:>10}",
            r.table_name, r.total_entries, r.unique_keys,
            r.avg_versions_per_key, r.distinct_blocks,
            human_bytes(r.exact_flat_key_bytes), human_bytes(r.exact_flat_value_bytes));
        total_flat_keys += r.exact_flat_key_bytes;
        total_flat_vals += r.exact_flat_value_bytes;
        total_unique += r.unique_keys;
        total_entries += r.total_entries;
        if r.distinct_blocks > total_distinct_blocks {
            total_distinct_blocks = r.distinct_blocks;
        }
    }

    let exact_flat_total = total_flat_keys + total_flat_vals;
    println!();
    println!("    Total unique keys:     {total_unique}");
    println!("    Total entries:         {total_entries}");
    println!("    Distinct blocks:       {total_distinct_blocks}");
    println!("    Exact flat data:       {} (keys: {} + values: {})",
        human_bytes(exact_flat_total), human_bytes(total_flat_keys), human_bytes(total_flat_vals));
    println!();

    // Key hotness.
    println!("  ── KEY HOTNESS ───────────────────────────────────────────────────");
    println!();
    for r in &a.deep_scans {
        if !r.top_hot_key_counts.is_empty() {
            println!("    {}: hottest key has {} versions, top-10: {:?}",
                r.table_name, r.max_versions_single_key, r.top_hot_key_counts);
        }
    }
    println!();

    // History index estimate.
    let estimated_history_bytes: u64 = a.deep_scans.iter().map(|r| {
        let avg_blocks_per_key = if r.unique_keys > 0 {
            r.total_entries as f64 / r.unique_keys as f64
        } else {
            0.0
        };
        (avg_blocks_per_key * 2.0 * r.unique_keys as f64) as u64
    }).sum();
    println!("  ── HISTORY INDEX ESTIMATE ─────────────────────────────────────────");
    println!();
    println!("    Roaring bitmap estimate:  {} (2 bytes per block per key)",
        human_bytes(estimated_history_bytes));
    println!();

    // Changeset projection table for multiple retention windows.
    let retention_windows: &[u64] = &[0, 10, 100, 1_000, 10_000, 100_000, 1_000_000];

    println!("  ── CHANGESET PROJECTIONS (by retention window) ─────────────────");
    println!();
    println!("    {:>12} {:>12} {:>12} {:>12} {:>12} {:>7}",
        "Retention", "CS Entries", "CS Size", "Total MDBX", "Savings", "Sav %");
    println!("    {:>12} {:>12} {:>12} {:>12} {:>12} {:>7}",
        "────────", "──────────", "───────", "──────────", "───────", "─────");

    for &window in retention_windows {
        let (cs_entries, cs_bytes) = changeset_bytes_for_window(&a.deep_scans, window);
        let projected = a.retained_size + exact_flat_total + cs_bytes + estimated_history_bytes;
        let savings = if a.total_db_size > projected { a.total_db_size - projected } else { 0 };
        let savings_pct = pct(savings, a.total_db_size);
        println!("    {:>12} {:>12} {:>12} {:>12} {:>12} {:>6.1}%",
            format_window(window),
            cs_entries,
            human_bytes(cs_bytes),
            human_bytes(projected),
            human_bytes(savings),
            savings_pct);
    }
    println!();
    println!("    Breakdown: retained {} + flat {} + history ~{}",
        human_bytes(a.retained_size), human_bytes(exact_flat_total),
        human_bytes(estimated_history_bytes));
    println!("    Current MDBX: {}", human_bytes(a.total_db_size));

    if a.mmap_total_size > 0 {
        println!();
        println!("    Note: mmap files add {} (unchanged by flat state)",
            human_bytes(a.mmap_total_size));
    }

    // Growth rate.
    if let Some(height) = a.block_height {
        if height > 0 && total_entries > 0 {
            let current_per_block = a.versioned_size as f64 / height as f64;
            println!();
            println!("  ── GROWTH RATE ───────────────────────────────────────────────────");
            println!();
            println!("    Current (versioned):  {:.0} bytes/block ({:.2} KB/block)",
                current_per_block, current_per_block / 1024.0);

            // Steady-state with pruning: net growth = 0 for MDBX state tables
            // (each block adds changeset entries and prunes old ones).
            // Only non-state tables grow: events, state_diffs, etc.
            let retained_per_block = a.retained_size as f64 / height as f64;
            println!("    Retained (events etc): {:.0} bytes/block ({:.2} KB/block)",
                retained_per_block, retained_per_block / 1024.0);
            println!("    With pruning: state tables stop growing (changeset in = pruned out)");
            println!("    Net growth ≈ retained growth only: {:.2} KB/block",
                retained_per_block / 1024.0);
        }
    }
    println!();
}

fn format_window(blocks: u64) -> String {
    if blocks == 0 {
        "0 (none)".to_string()
    } else if blocks >= 1_000_000 {
        format!("{}M", blocks / 1_000_000)
    } else if blocks >= 1_000 {
        format!("{}K", blocks / 1_000)
    } else {
        blocks.to_string()
    }
}

fn print_json_entry(a: &DbAnalysis) {
    println!("  {{");
    println!("    \"path\": \"{}\",", a.label);
    println!("    \"total_db_bytes\": {},", a.total_db_size);
    println!("    \"mmap_total_bytes\": {},", a.mmap_total_size);
    if let Some(h) = a.block_height {
        println!("    \"block_height\": {h},");
    }
    println!("    \"versioned_bytes\": {},", a.versioned_size);
    println!("    \"retained_bytes\": {},", a.retained_size);
    if !a.quick_scans.is_empty() {
        println!("    \"quick_scan\": [");
        for (i, r) in a.quick_scans.iter().enumerate() {
            let c = if i < a.quick_scans.len() - 1 { "," } else { "" };
            println!("      {{ \"table\": \"{}\", \"unique_keys\": {}, \"entries\": {}, \"avg_versions\": {:.1}, \"estimated_flat_bytes\": {} }}{c}",
                r.table_name, r.unique_keys, r.total_entries, r.avg_versions_per_key, r.estimated_flat_bytes);
        }
        println!("    ],");
    }
    if !a.deep_scans.is_empty() {
        println!("    \"deep_scan\": [");
        for (i, r) in a.deep_scans.iter().enumerate() {
            let c = if i < a.deep_scans.len() - 1 { "," } else { "" };
            println!("      {{ \"table\": \"{}\", \"unique_keys\": {}, \"entries\": {}, \"avg_versions\": {:.1}, \
                \"exact_flat_key_bytes\": {}, \"exact_flat_value_bytes\": {}, \
                \"distinct_blocks\": {}, \"avg_keys_per_block\": {:.1}, \
                \"max_versions_single_key\": {}, \"total_value_bytes\": {} }}{c}",
                r.table_name, r.unique_keys, r.total_entries, r.avg_versions_per_key,
                r.exact_flat_key_bytes, r.exact_flat_value_bytes,
                r.distinct_blocks, r.avg_keys_per_block,
                r.max_versions_single_key, r.total_value_bytes);
        }
        println!("    ],");
    }
    println!("    \"tables\": {{");
    let entries: Vec<_> = a.table_stats.iter().collect();
    for (i, (name, s)) in entries.iter().enumerate() {
        let c = if i < entries.len() - 1 { "," } else { "" };
        println!("      \"{name}\": {{ \"entries\": {}, \"total_bytes\": {} }}{c}", s.entries, s.total_size);
    }
    println!("    }}");
    print!("  }}");
}

fn print_combined_summary(analyses: &[DbAnalysis]) {
    println!();
    println!("╔══════════════════════════════════════════════════════════════════════╗");
    println!("║                       COMBINED SUMMARY                              ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝");
    println!();

    let total_current: u64 = analyses.iter().map(|a| a.total_db_size).sum();
    let total_versioned: u64 = analyses.iter().map(|a| a.versioned_size).sum();
    let total_retained: u64 = analyses.iter().map(|a| a.retained_size).sum();

    println!("  {:<30} {:>12} {:>12} {:>12}", "Database", "Total", "Versioned", "Retained");
    println!("  {:<30} {:>12} {:>12} {:>12}", "────────", "─────", "─────────", "────────");
    for a in analyses {
        println!("  {:<30} {:>12} {:>12} {:>12}",
            a.label, human_bytes(a.total_db_size), human_bytes(a.versioned_size), human_bytes(a.retained_size));
    }
    println!("  {:<30} {:>12} {:>12} {:>12}",
        "COMBINED", human_bytes(total_current), human_bytes(total_versioned), human_bytes(total_retained));
    println!();

    let has_deep = analyses.iter().any(|a| !a.deep_scans.is_empty());
    let has_quick = analyses.iter().any(|a| !a.quick_scans.is_empty());

    if has_deep {
        let total_flat: u64 = analyses.iter().flat_map(|a| a.deep_scans.iter())
            .map(|r| r.exact_flat_key_bytes + r.exact_flat_value_bytes).sum();
        let projected = total_retained + total_flat;
        println!("  Deep scan combined:");
        println!("    Exact flat data (all DBs):  {}", human_bytes(total_flat));
        println!("    Projected minimum:          {}", human_bytes(projected));
        println!("    Current:                    {}", human_bytes(total_current));
        if total_current > projected {
            println!("    Max savings:                {} ({:.1}%)",
                human_bytes(total_current - projected), pct(total_current - projected, total_current));
        }
    } else if has_quick {
        let total_flat: u64 = analyses.iter().flat_map(|a| a.quick_scans.iter())
            .map(|r| r.estimated_flat_bytes).sum();
        let projected = total_retained + total_flat;
        println!("  Quick scan combined:");
        println!("    Estimated flat (all DBs):  {}", human_bytes(total_flat));
        println!("    Projected minimum:         {}", human_bytes(projected));
        println!("    Current:                   {}", human_bytes(total_current));
        if total_current > projected {
            println!("    Max savings:               {} ({:.1}%)",
                human_bytes(total_current - projected), pct(total_current - projected, total_current));
        }
    }
}
