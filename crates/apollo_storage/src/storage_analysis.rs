//! Storage analysis module for measuring optimization opportunities on a populated MDBX database.
#![allow(clippy::as_conversions)]

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Serialize;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::db::db_stats::DbWholeStats;
use crate::db::table_types::{DbCursorTrait, Table};
use crate::mmap_file::LocationInFile;
use crate::{
    open_storage_read_only,
    OffsetKind,
    StorageConfig,
    StorageReader,
    StorageResult,
    StorageScope,
};

/// Configuration for the analysis run.
pub struct AnalysisConfig {
    /// Number of mmap entries to sample for compression analysis.
    pub mmap_sample_count: usize,
    /// Optional second DB path for duplication comparison.
    pub compare_db_path: Option<PathBuf>,
    /// Skip full table iteration (M2/M3) for a quick overview.
    pub skip_full_iteration: bool,
}

// ----- Report structs -----

/// Top-level analysis report.
#[derive(Serialize)]
pub struct AnalysisReport {
    /// Per-table and global size overview.
    pub overview: TableOverview,
    /// Flat state savings analysis (None if skipped).
    pub flat_state: Option<FlatStateAnalysis>,
    /// Varint Felt encoding savings (None if skipped).
    pub varint_felt: Option<VarintFeltAnalysis>,
    /// Mmap file compression analysis.
    pub mmap_compression: Vec<MmapFileCompressionResult>,
    /// Duplication between two DBs (None if no compare path).
    pub duplication: Option<DuplicationAnalysis>,
    /// Total bytes wasted by VersionZeroWrapper (1 byte per value entry).
    pub version_wrapper_overhead_bytes: u64,
    /// Total bytes used by ThinStateDiff mmap file.
    pub thin_state_diff_mmap_bytes: u64,
}

/// Overview of table sizes.
#[derive(Serialize)]
pub struct TableOverview {
    /// Global database statistics.
    pub db_stats: DbWholeStats,
    /// Per-table statistics sorted by size descending.
    pub tables: Vec<TableEntry>,
}

/// A single table's statistics.
#[derive(Serialize)]
pub struct TableEntry {
    /// Table name.
    pub name: String,
    /// Number of entries.
    pub entries: usize,
    /// Total size in bytes.
    pub total_size: u64,
    /// Percentage of total DB size.
    pub pct: f64,
}

/// Flat state savings for all state tables.
#[derive(Serialize)]
pub struct FlatStateAnalysis {
    /// Per-table results.
    pub tables: Vec<FlatStateTableResult>,
}

/// Flat state savings for a single table.
#[derive(Serialize)]
pub struct FlatStateTableResult {
    /// Table name.
    pub table_name: String,
    /// Total entries in the table.
    pub total_entries: u64,
    /// Number of distinct prefix keys (without BlockNumber).
    pub distinct_keys: u64,
    /// Average versions per key.
    pub avg_versions: f64,
    /// Percentage of entries that would be eliminated.
    pub reduction_pct: f64,
}

/// Varint encoding savings analysis.
#[derive(Serialize)]
pub struct VarintFeltAnalysis {
    /// Per-table results.
    pub tables: Vec<VarintTableResult>,
}

/// Varint savings for a single table's values.
#[derive(Serialize)]
pub struct VarintTableResult {
    /// Table name.
    pub table_name: String,
    /// Number of values analyzed.
    pub total_values: u64,
    /// Values that fit in 1-4 bytes.
    pub fits_4_bytes: u64,
    /// Values that fit in 5-8 bytes.
    pub fits_8_bytes: u64,
    /// Values that fit in 9-16 bytes.
    pub fits_16_bytes: u64,
    /// Values that need 17-32 bytes.
    pub full_32_bytes: u64,
    /// Average bytes needed per value.
    pub avg_bytes_needed: f64,
    /// Percentage saved vs fixed 32 bytes.
    pub savings_pct: f64,
    /// Average nanoseconds to varint-encode one value.
    pub avg_encode_ns: f64,
    /// Average nanoseconds to varint-decode one value.
    pub avg_decode_ns: f64,
}

/// Mmap compression result for a single file type.
#[derive(Serialize)]
pub struct MmapFileCompressionResult {
    /// File type name.
    pub file_type: String,
    /// Number of entries sampled.
    pub samples: usize,
    /// Average original size in bytes.
    pub avg_original_bytes: f64,
    /// Average compressed size in bytes.
    pub avg_compressed_bytes: f64,
    /// Compression ratio (compressed / original).
    pub ratio: f64,
    /// Percentage saved.
    pub savings_pct: f64,
    /// Average nanoseconds to compress one entry.
    pub avg_compress_ns: f64,
    /// Average nanoseconds to decompress one entry.
    pub avg_decompress_ns: f64,
}

/// Duplication analysis between two DBs.
#[derive(Serialize)]
pub struct DuplicationAnalysis {
    /// Per-table comparison.
    pub tables: Vec<DuplicationTableResult>,
}

/// Duplication for a single table.
#[derive(Serialize)]
pub struct DuplicationTableResult {
    /// Table name.
    pub table_name: String,
    /// Entries in the primary DB.
    pub primary_entries: usize,
    /// Entries in the comparison DB.
    pub compare_entries: usize,
    /// Size in primary DB.
    pub primary_size: u64,
    /// Size in comparison DB.
    pub compare_size: u64,
}

// ----- Analysis implementation -----

/// Run all analysis measurements on the given reader.
pub fn run_analysis(
    reader: &StorageReader,
    config: &AnalysisConfig,
) -> StorageResult<AnalysisReport> {
    let overview = measure_table_overview(reader)?;

    let (flat_state, varint_felt) = if config.skip_full_iteration {
        (None, None)
    } else {
        let (fs, vf) = measure_flat_state_and_varint(reader)?;
        (Some(fs), Some(vf))
    };

    let mmap_compression = measure_mmap_compression(reader, config.mmap_sample_count)?;

    let duplication = match &config.compare_db_path {
        Some(path) => Some(measure_duplication(reader, path)?),
        None => None,
    };

    let version_wrapper_overhead_bytes = measure_version_wrapper_overhead(&overview);
    let thin_state_diff_mmap_bytes = measure_thin_state_diff_mmap_size(reader)?;

    Ok(AnalysisReport {
        overview,
        flat_state,
        varint_felt,
        mmap_compression,
        duplication,
        version_wrapper_overhead_bytes,
        thin_state_diff_mmap_bytes,
    })
}

// ----- M1: Table Overview -----

fn measure_table_overview(reader: &StorageReader) -> StorageResult<TableOverview> {
    let stats = reader.db_tables_stats()?;
    let mut tables: Vec<TableEntry> = stats
        .tables_stats
        .into_iter()
        .map(|(name, stat)| TableEntry {
            name,
            entries: stat.entries,
            total_size: stat.total_size,
            pct: stat.db_portion * 100.0,
        })
        .collect();
    tables.sort_by(|a, b| b.total_size.cmp(&a.total_size));
    Ok(TableOverview { db_stats: stats.db_stats, tables })
}

// ----- M2+M3: Flat State Savings + Varint Felt (combined pass) -----

fn measure_flat_state_and_varint(
    reader: &StorageReader,
) -> StorageResult<(FlatStateAnalysis, VarintFeltAnalysis)> {
    let mut flat_state_results = Vec::new();
    let mut varint_results = Vec::new();

    // contract_storage: CommonPrefix with main_key = (ContractAddress, StorageKey)
    {
        let txn = reader.begin_ro_txn()?;
        let table = txn.open_table(&txn.tables.contract_storage)?;
        let mut cursor = table.cursor(&txn.txn)?;

        let mut total_entries: u64 = 0;
        let mut distinct_keys: u64 = 0;
        let mut prev_prefix: Option<(ContractAddress, StorageKey)> = None;
        let mut varint_buckets = VarintBuckets::default();

        let mut current = cursor.next()?;
        while let Some(((prefix, _block_number), value)) = current {
            total_entries += 1;
            if prev_prefix.as_ref() != Some(&prefix) {
                distinct_keys += 1;
                prev_prefix = Some(prefix);
            }
            varint_buckets.record_felt(&value);
            if total_entries.is_multiple_of(1_000_000) {
                eprintln!("  contract_storage: {total_entries} entries processed...");
            }
            current = cursor.next()?;
        }

        flat_state_results.push(flat_state_result(
            "contract_storage",
            total_entries,
            distinct_keys,
        ));
        varint_results.push(varint_buckets.into_result("contract_storage"));
    }

    // nonces: CommonPrefix with main_key = ContractAddress
    {
        let txn = reader.begin_ro_txn()?;
        let table = txn.open_table(&txn.tables.nonces)?;
        let mut cursor = table.cursor(&txn.txn)?;

        let mut total_entries: u64 = 0;
        let mut distinct_keys: u64 = 0;
        let mut prev_prefix: Option<ContractAddress> = None;
        let mut varint_buckets = VarintBuckets::default();

        let mut current = cursor.next()?;
        while let Some(((prefix, _block_number), value)) = current {
            total_entries += 1;
            if prev_prefix.as_ref() != Some(&prefix) {
                distinct_keys += 1;
                prev_prefix = Some(prefix);
            }
            varint_buckets.record_nonce(&value);
            if total_entries.is_multiple_of(1_000_000) {
                eprintln!("  nonces: {total_entries} entries processed...");
            }
            current = cursor.next()?;
        }

        flat_state_results.push(flat_state_result("nonces", total_entries, distinct_keys));
        varint_results.push(varint_buckets.into_result("nonces"));
    }

    // deployed_contracts: SimpleTable with key = (ContractAddress, BlockNumber)
    {
        let txn = reader.begin_ro_txn()?;
        let table = txn.open_table(&txn.tables.deployed_contracts)?;
        let mut cursor = table.cursor(&txn.txn)?;

        let mut total_entries: u64 = 0;
        let mut distinct_keys: u64 = 0;
        let mut prev_prefix: Option<ContractAddress> = None;

        let mut current = cursor.next()?;
        while let Some(((address, _block_number), _value)) = current {
            total_entries += 1;
            if prev_prefix.as_ref() != Some(&address) {
                distinct_keys += 1;
                prev_prefix = Some(address);
            }
            if total_entries.is_multiple_of(1_000_000) {
                eprintln!("  deployed_contracts: {total_entries} entries processed...");
            }
            current = cursor.next()?;
        }

        flat_state_results.push(flat_state_result(
            "deployed_contracts",
            total_entries,
            distinct_keys,
        ));
    }

    // compiled_class_hash: CommonPrefix with main_key = ClassHash
    {
        let txn = reader.begin_ro_txn()?;
        let table = txn.open_table(&txn.tables.compiled_class_hash)?;
        let mut cursor = table.cursor(&txn.txn)?;

        let mut total_entries: u64 = 0;
        let mut distinct_keys: u64 = 0;
        let mut prev_prefix: Option<starknet_api::core::ClassHash> = None;

        let mut current = cursor.next()?;
        while let Some(((prefix, _block_number), _value)) = current {
            total_entries += 1;
            if prev_prefix.as_ref() != Some(&prefix) {
                distinct_keys += 1;
                prev_prefix = Some(prefix);
            }
            current = cursor.next()?;
        }

        flat_state_results.push(flat_state_result(
            "compiled_class_hash",
            total_entries,
            distinct_keys,
        ));
    }

    Ok((
        FlatStateAnalysis { tables: flat_state_results },
        VarintFeltAnalysis { tables: varint_results },
    ))
}

fn flat_state_result(
    table_name: &str,
    total_entries: u64,
    distinct_keys: u64,
) -> FlatStateTableResult {
    let avg_versions =
        if distinct_keys > 0 { total_entries as f64 / distinct_keys as f64 } else { 0.0 };
    let reduction_pct = if total_entries > 0 {
        (1.0 - distinct_keys as f64 / total_entries as f64) * 100.0
    } else {
        0.0
    };
    FlatStateTableResult {
        table_name: table_name.to_string(),
        total_entries,
        distinct_keys,
        avg_versions,
        reduction_pct,
    }
}

#[derive(Default)]
struct VarintBuckets {
    total: u64,
    fits_4: u64,
    fits_8: u64,
    fits_16: u64,
    full_32: u64,
    total_bytes_needed: u64,
    total_encode_ns: u64,
    total_decode_ns: u64,
}

impl VarintBuckets {
    fn record_felt(&mut self, felt: &Felt) {
        let bytes = felt.to_bytes_be();
        self.record_bytes(&bytes);
    }

    fn record_nonce(&mut self, nonce: &Nonce) {
        let bytes = nonce.0.to_bytes_be();
        self.record_bytes(&bytes);
    }

    fn record_bytes(&mut self, bytes: &[u8; 32]) {
        let leading_zeros = bytes.iter().take_while(|&&b| b == 0).count();
        let needed = (32 - leading_zeros).max(1) as u64;
        self.total += 1;

        // Benchmark varint encode/decode: strip leading zeros, then reconstruct.
        let trimmed = &bytes[32 - needed as usize..];

        let encode_start = Instant::now();
        let mut encoded = Vec::with_capacity(33);
        encoded.push(needed as u8);
        encoded.extend_from_slice(trimmed);
        self.total_encode_ns += encode_start.elapsed().as_nanos() as u64;

        let decode_start = Instant::now();
        let len = encoded[0] as usize;
        let mut reconstructed = [0u8; 32];
        reconstructed[32 - len..].copy_from_slice(&encoded[1..1 + len]);
        std::hint::black_box(&reconstructed);
        self.total_decode_ns += decode_start.elapsed().as_nanos() as u64;
        self.total_bytes_needed += needed;
        match needed {
            1..=4 => self.fits_4 += 1,
            5..=8 => self.fits_8 += 1,
            9..=16 => self.fits_16 += 1,
            _ => self.full_32 += 1,
        }
    }

    fn into_result(self, table_name: &str) -> VarintTableResult {
        let avg_bytes_needed =
            if self.total > 0 { self.total_bytes_needed as f64 / self.total as f64 } else { 0.0 };
        let savings_pct =
            if self.total > 0 { (1.0 - avg_bytes_needed / 32.0) * 100.0 } else { 0.0 };
        let avg_encode_ns =
            if self.total > 0 { self.total_encode_ns as f64 / self.total as f64 } else { 0.0 };
        let avg_decode_ns =
            if self.total > 0 { self.total_decode_ns as f64 / self.total as f64 } else { 0.0 };
        VarintTableResult {
            table_name: table_name.to_string(),
            total_values: self.total,
            fits_4_bytes: self.fits_4,
            fits_8_bytes: self.fits_8,
            fits_16_bytes: self.fits_16,
            full_32_bytes: self.full_32,
            avg_bytes_needed,
            savings_pct,
            avg_encode_ns,
            avg_decode_ns,
        }
    }
}

// ----- M4: Mmap Compression -----

fn measure_mmap_compression(
    reader: &StorageReader,
    sample_count: usize,
) -> StorageResult<Vec<MmapFileCompressionResult>> {
    let mut results = Vec::new();

    // State diffs
    if let Ok(result) = measure_single_mmap_compression(reader, "thin_state_diff", sample_count) {
        results.push(result);
    }

    // Classes
    if let Ok(result) = measure_single_mmap_compression(reader, "contract_class", sample_count) {
        results.push(result);
    }

    // CASMs
    if let Ok(result) = measure_single_mmap_compression(reader, "casm", sample_count) {
        results.push(result);
    }

    // Deprecated classes
    if let Ok(result) =
        measure_single_mmap_compression(reader, "deprecated_contract_class", sample_count)
    {
        results.push(result);
    }

    // Transaction outputs (may not exist in StateOnly)
    if reader.get_scope() != StorageScope::StateOnly {
        if let Ok(result) =
            measure_single_mmap_compression(reader, "transaction_output", sample_count)
        {
            results.push(result);
        }

        if let Ok(result) = measure_single_mmap_compression(reader, "transaction", sample_count) {
            results.push(result);
        }
    }

    Ok(results)
}

fn measure_single_mmap_compression(
    reader: &StorageReader,
    file_type: &str,
    sample_count: usize,
) -> StorageResult<MmapFileCompressionResult> {
    let txn = reader.begin_ro_txn()?;

    // Collect LocationInFile entries from the appropriate table.
    let locations: Vec<LocationInFile> = match file_type {
        "thin_state_diff" => {
            let table = txn.open_table(&txn.tables.state_diffs)?;
            let mut cursor = table.cursor(&txn.txn)?;
            collect_locations_from_cursor(&mut cursor, sample_count)?
        }
        "contract_class" => {
            let table = txn.open_table(&txn.tables.declared_classes)?;
            let mut cursor = table.cursor(&txn.txn)?;
            collect_locations_from_cursor(&mut cursor, sample_count)?
        }
        "casm" => {
            let table = txn.open_table(&txn.tables.casms)?;
            let mut cursor = table.cursor(&txn.txn)?;
            collect_locations_from_cursor(&mut cursor, sample_count)?
        }
        "deprecated_contract_class" => {
            let table = txn.open_table(&txn.tables.deprecated_declared_classes)?;
            let mut cursor = table.cursor(&txn.txn)?;
            collect_indexed_locations_from_cursor(&mut cursor, sample_count)?
        }
        "transaction_output" | "transaction" => {
            let table = txn.open_table(&txn.tables.transaction_metadata)?;
            let mut cursor = table.cursor(&txn.txn)?;
            collect_tx_metadata_locations(&mut cursor, sample_count, file_type)?
        }
        _ => return Ok(empty_mmap_result(file_type)),
    };

    if locations.is_empty() {
        return Ok(empty_mmap_result(file_type));
    }

    // Read raw bytes, compress, and measure timing.
    let mut total_original: u64 = 0;
    let mut total_compressed: u64 = 0;
    let mut total_compress_ns: u64 = 0;
    let mut total_decompress_ns: u64 = 0;
    let mut sampled = 0;

    for location in &locations {
        let raw_bytes = match file_type {
            "thin_state_diff" => txn.file_handlers.thin_state_diff.get_raw_bytes(*location),
            "contract_class" => txn.file_handlers.contract_class.get_raw_bytes(*location),
            "casm" => txn.file_handlers.casm.get_raw_bytes(*location),
            "deprecated_contract_class" => {
                txn.file_handlers.deprecated_contract_class.get_raw_bytes(*location)
            }
            "transaction_output" => txn.file_handlers.transaction_output.get_raw_bytes(*location),
            "transaction" => txn.file_handlers.transaction.get_raw_bytes(*location),
            _ => continue,
        };

        if let Ok(bytes) = raw_bytes {
            let original_len = bytes.len() as u64;

            let compress_start = Instant::now();
            let compressed = match zstd::encode_all(bytes.as_slice(), 3) {
                Ok(c) => c,
                Err(_) => continue,
            };
            total_compress_ns += compress_start.elapsed().as_nanos() as u64;

            let decompress_start = Instant::now();
            let _decompressed = zstd::decode_all(compressed.as_slice());
            total_decompress_ns += decompress_start.elapsed().as_nanos() as u64;

            total_original += original_len;
            total_compressed += compressed.len() as u64;
            sampled += 1;
        }
    }

    if sampled == 0 {
        return Ok(empty_mmap_result(file_type));
    }

    let avg_original = total_original as f64 / sampled as f64;
    let avg_compressed = total_compressed as f64 / sampled as f64;
    let ratio =
        if total_original > 0 { total_compressed as f64 / total_original as f64 } else { 1.0 };

    Ok(MmapFileCompressionResult {
        file_type: file_type.to_string(),
        samples: sampled,
        avg_original_bytes: avg_original,
        avg_compressed_bytes: avg_compressed,
        ratio,
        savings_pct: (1.0 - ratio) * 100.0,
        avg_compress_ns: total_compress_ns as f64 / sampled as f64,
        avg_decompress_ns: total_decompress_ns as f64 / sampled as f64,
    })
}

fn empty_mmap_result(file_type: &str) -> MmapFileCompressionResult {
    MmapFileCompressionResult {
        file_type: file_type.to_string(),
        samples: 0,
        avg_original_bytes: 0.0,
        avg_compressed_bytes: 0.0,
        ratio: 0.0,
        savings_pct: 0.0,
        avg_compress_ns: 0.0,
        avg_decompress_ns: 0.0,
    }
}

/// Collect LocationInFile values from a cursor over a table that stores LocationInFile as value.
fn collect_locations_from_cursor<K, C>(
    cursor: &mut C,
    max_samples: usize,
) -> StorageResult<Vec<LocationInFile>>
where
    C: DbCursorTrait<Key = K, Value = crate::db::serialization::VersionZeroWrapper<LocationInFile>>,
{
    let mut locations = Vec::new();
    let mut current = cursor.next()?;
    while let Some((_key, location)) = current {
        locations.push(location);
        current = cursor.next()?;
    }
    Ok(evenly_sample(locations, max_samples))
}

/// Collect LocationInFile from deprecated_declared_classes (IndexedDeprecatedContractClass).
fn collect_indexed_locations_from_cursor<K, C>(
    cursor: &mut C,
    max_samples: usize,
) -> StorageResult<Vec<LocationInFile>>
where
    C: DbCursorTrait<
            Key = K,
            Value = crate::db::serialization::VersionZeroWrapper<
                crate::IndexedDeprecatedContractClass,
            >,
        >,
{
    let mut locations = Vec::new();
    let mut current = cursor.next()?;
    while let Some((_key, indexed)) = current {
        locations.push(indexed.location_in_file);
        current = cursor.next()?;
    }
    Ok(evenly_sample(locations, max_samples))
}

/// Collect LocationInFile from transaction_metadata for either tx or tx_output.
fn collect_tx_metadata_locations<K, C>(
    cursor: &mut C,
    max_samples: usize,
    file_type: &str,
) -> StorageResult<Vec<LocationInFile>>
where
    C: DbCursorTrait<
            Key = K,
            Value = crate::db::serialization::VersionZeroWrapper<crate::TransactionMetadata>,
        >,
{
    let mut locations = Vec::new();
    let mut current = cursor.next()?;
    while let Some((_key, metadata)) = current {
        let location = if file_type == "transaction" {
            metadata.tx_location
        } else {
            metadata.tx_output_location
        };
        locations.push(location);
        current = cursor.next()?;
    }
    Ok(evenly_sample(locations, max_samples))
}

fn evenly_sample<T>(mut items: Vec<T>, max_samples: usize) -> Vec<T> {
    let total = items.len();
    if total <= max_samples {
        return items;
    }
    let step = total as f64 / max_samples as f64;
    let indices: Vec<usize> = (0..max_samples).map(|i| (i as f64 * step) as usize).collect();
    // Extract in reverse order to avoid index shifting.
    let mut result: Vec<T> = Vec::with_capacity(max_samples);
    for &idx in indices.iter().rev() {
        result.push(items.swap_remove(idx));
    }
    result.reverse();
    result
}

// ----- M5: Duplication Analysis -----

fn measure_duplication(
    reader: &StorageReader,
    compare_path: &Path,
) -> StorageResult<DuplicationAnalysis> {
    // Parse the compare path the same way as the primary.
    let chain_id = compare_path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "SN_MAIN".to_string());
    let path_prefix = compare_path.parent().unwrap_or(compare_path).to_path_buf();

    let compare_config = StorageConfig {
        db_config: crate::db::DbConfig {
            path_prefix,
            chain_id: starknet_api::core::ChainId::Other(chain_id),
            enforce_file_exists: true,
            min_size: 1 << 20,
            max_size: 1 << 40,
            growth_step: 1 << 32,
            max_readers: 1 << 13,
        },
        mmap_file_config: crate::mmap_file::MmapFileConfig::default(),
        scope: StorageScope::FullArchive,
    };
    let compare_reader = open_storage_read_only(compare_config)?;

    let primary_stats = reader.db_tables_stats()?;
    let compare_stats = compare_reader.db_tables_stats()?;

    let state_tables =
        ["contract_storage", "nonces", "deployed_contracts", "compiled_class_hash", "state_diffs"];

    let mut tables = Vec::new();
    for table_name in &state_tables {
        let name = table_name.to_string();
        let primary = primary_stats.tables_stats.get(&name);
        let compare = compare_stats.tables_stats.get(&name);
        if let (Some(p), Some(c)) = (primary, compare) {
            tables.push(DuplicationTableResult {
                table_name: name,
                primary_entries: p.entries,
                compare_entries: c.entries,
                primary_size: p.total_size,
                compare_size: c.total_size,
            });
        }
    }

    Ok(DuplicationAnalysis { tables })
}

// ----- M6: VersionZeroWrapper Overhead -----

const VERSION_ZERO_WRAPPER_TABLES: &[&str] = &[
    "block_signatures",
    "casms",
    "declared_classes",
    "deprecated_declared_classes",
    "deployed_contracts",
    "headers",
    "last_voted_marker",
    "markers",
    "nonces",
    "partial_block_hashes_components",
    "state_diffs",
    "transaction_metadata",
    "block_hashes",
    "starknet_version",
    "compiled_class_hash",
];

fn measure_version_wrapper_overhead(overview: &TableOverview) -> u64 {
    overview
        .tables
        .iter()
        .filter(|t| VERSION_ZERO_WRAPPER_TABLES.contains(&t.name.as_str()))
        .map(|t| t.entries as u64)
        .sum()
}

// ----- M7: ThinStateDiff Mmap Size -----

fn measure_thin_state_diff_mmap_size(reader: &StorageReader) -> StorageResult<u64> {
    let txn = reader.begin_ro_txn()?;
    let offset = txn.get_file_offset(OffsetKind::ThinStateDiff)?;
    Ok(offset.unwrap_or(0) as u64)
}

// ----- Human-readable summary -----

impl AnalysisReport {
    /// Format a human-readable summary of the analysis.
    pub fn human_readable_summary(&self) -> String {
        let mut out = String::new();
        out.push_str("=== Storage Analysis Report ===\n");
        out.push_str(&format!(
            "Total size: {} ({} pages, freelist: {} pages)\n\n",
            human_bytes(self.overview.db_stats.total_size),
            self.overview.db_stats.leaf_pages
                + self.overview.db_stats.branch_pages
                + self.overview.db_stats.overflow_pages,
            self.overview.db_stats.freelist_size,
        ));

        out.push_str("Top tables by size:\n");
        for table in self.overview.tables.iter().take(10) {
            out.push_str(&format!(
                "  {:<40} {:>12}  ({:>5.1}%)  {:>10} entries\n",
                table.name,
                human_bytes(table.total_size),
                table.pct,
                table.entries,
            ));
        }
        out.push('\n');

        if let Some(flat_state) = &self.flat_state {
            out.push_str("Flat state savings:\n");
            for table in &flat_state.tables {
                out.push_str(&format!(
                    "  {}: {} entries → {} distinct keys ({:.2} avg versions, {:.1}% reduction)\n",
                    table.table_name,
                    table.total_entries,
                    table.distinct_keys,
                    table.avg_versions,
                    table.reduction_pct,
                ));
            }
            out.push('\n');
        }

        if let Some(varint) = &self.varint_felt {
            out.push_str("Varint Felt savings:\n");
            for table in &varint.tables {
                out.push_str(&format!(
                    "  {} values: avg {:.1} bytes needed (vs 32 fixed) → {:.1}% savings (encode: \
                     {:.0}ns, decode: {:.0}ns per value)\n",
                    table.table_name,
                    table.avg_bytes_needed,
                    table.savings_pct,
                    table.avg_encode_ns,
                    table.avg_decode_ns,
                ));
            }
            out.push('\n');
        }

        if !self.mmap_compression.is_empty() {
            out.push_str("Mmap compression (zstd level 3):\n");
            for result in &self.mmap_compression {
                if result.samples > 0 {
                    out.push_str(&format!(
                        "  {}: {:.1}x ratio ({:.1}% savings, {} samples, avg {:.0}B → {:.0}B, \
                         compress: {:.0}ns, decompress: {:.0}ns)\n",
                        result.file_type,
                        if result.ratio > 0.0 { 1.0 / result.ratio } else { 0.0 },
                        result.savings_pct,
                        result.samples,
                        result.avg_original_bytes,
                        result.avg_compressed_bytes,
                        result.avg_compress_ns,
                        result.avg_decompress_ns,
                    ));
                }
            }
            out.push('\n');
        }

        if let Some(dup) = &self.duplication {
            out.push_str("Duplication analysis (state tables):\n");
            for table in &dup.tables {
                out.push_str(&format!(
                    "  {}: primary={} entries ({}), compare={} entries ({})\n",
                    table.table_name,
                    table.primary_entries,
                    human_bytes(table.primary_size),
                    table.compare_entries,
                    human_bytes(table.compare_size),
                ));
            }
            out.push('\n');
        }

        out.push_str(&format!(
            "VersionZeroWrapper overhead: {}\n",
            human_bytes(self.version_wrapper_overhead_bytes)
        ));
        out.push_str(&format!(
            "ThinStateDiff mmap size: {}\n",
            human_bytes(self.thin_state_diff_mmap_bytes)
        ));

        out
    }
}

fn human_bytes(bytes: u64) -> String {
    if bytes >= 1 << 30 {
        format!("{:.1} GB", bytes as f64 / (1u64 << 30) as f64)
    } else if bytes >= 1 << 20 {
        format!("{:.1} MB", bytes as f64 / (1u64 << 20) as f64)
    } else if bytes >= 1 << 10 {
        format!("{:.1} KB", bytes as f64 / (1u64 << 10) as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
#[path = "storage_analysis_test.rs"]
mod storage_analysis_test;
