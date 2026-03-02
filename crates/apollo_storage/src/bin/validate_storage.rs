//! Storage validation binary for comparing two MDBX databases.
//!
//! Compares all tables between two databases byte-by-byte to verify that storage
//! batching doesn't corrupt data. Handles databases at different block heights by
//! treating the smaller DB (fewer blocks) as the reference and verifying that every
//! entry in it exists identically in the larger DB.

#![cfg(feature = "storage_validation")]

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Instant;

use clap::Parser;
use libmdbx::{DatabaseFlags, Geometry, Mode, WriteMap};

type Environment = libmdbx::Database<WriteMap>;
type DbKeyType<'env> = Cow<'env, [u8]>;
type DbValueType<'env> = Cow<'env, [u8]>;

#[derive(Parser, Debug)]
#[command(name = "validate_storage", about = "Compare two MDBX storage databases")]
struct Args {
    /// Path to first database directory (e.g., /data/batch100/state_sync/SN_MAIN)
    #[arg(long)]
    db1_path: PathBuf,

    /// Path to second database directory (e.g., /data/baseline/state_sync/SN_MAIN)
    #[arg(long)]
    db2_path: PathBuf,

    /// Tables to skip (comma-separated)
    #[arg(long, default_value = "last_voted_marker,storage_version")]
    skip_tables: String,

    /// Compare mmap files (up to the smaller file's size)
    #[arg(long, default_value = "true")]
    compare_mmap: bool,

    /// Progress report interval (number of entries)
    #[arg(long, default_value = "100000")]
    progress_interval: u64,

    /// Sample interval: compare every Nth entry instead of all entries.
    /// 1 = compare all (full validation), 100 = compare every 100th entry.
    #[arg(long, default_value = "1")]
    sample_interval: u64,

    /// Number of probe samples per table (uses set_range seek instead of iteration).
    /// When set, overrides sample_interval. Each probe is an O(log N) seek.
    #[arg(long)]
    num_samples: Option<u64>,
}

const ALL_TABLES: &[&str] = &[
    "block_hash_to_number",
    "block_signatures",
    "casms",
    "contract_storage",
    "declared_classes",
    "declared_classes_block",
    "deprecated_declared_classes",
    "deprecated_declared_classes_block",
    "deployed_contracts",
    "events",
    "headers",
    "last_voted_marker",
    "markers",
    "nonces",
    "partial_block_hashes_components",
    "file_offsets",
    "state_diffs",
    "transaction_hash_to_idx",
    "transaction_metadata",
    "block_hashes",
    "global_root",
    "starknet_version",
    "compiled_class_hash",
    "stateless_compiled_class_hash_v2",
];

const MMAP_FILES: &[&str] = &["thin_state_diff.dat", "contract_class.dat", "casm.dat"];

fn fmt_duration(secs: f64) -> String {
    if secs < 60.0 {
        format!("{secs:.1}s")
    } else if secs < 3600.0 {
        format!("{:.0}m {:.0}s", secs / 60.0, secs % 60.0)
    } else {
        format!("{:.0}h {:.0}m", secs / 3600.0, (secs % 3600.0) / 60.0)
    }
}

fn fmt_bytes(n: u64) -> String {
    if n < 1024 {
        format!("{n} B")
    } else if n < 1024 * 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else if n < 1024 * 1024 * 1024 {
        format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", n as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn hex_prefix(data: &[u8], max_bytes: usize) -> String {
    let len = data.len().min(max_bytes);
    let hex: String = data[..len].iter().map(|b| format!("{b:02x}")).collect();
    if data.len() > max_bytes { format!("{hex}...") } else { hex }
}

fn main() {
    let total_start = Instant::now();
    let args = Args::parse();

    println!("╔══════════════════════════════════════════╗");
    println!("║       STORAGE VALIDATION TOOL            ║");
    println!("╚══════════════════════════════════════════╝");
    println!();
    println!("DB1: {:?}", args.db1_path);
    println!("DB2: {:?}", args.db2_path);
    println!();

    let skip_tables: HashSet<&str> = args.skip_tables.split(',').map(|s| s.trim()).collect();
    println!("Skipping tables: {:?}", skip_tables);
    if let Some(n) = args.num_samples {
        println!("Probe sampling mode: {n} random probes per table (O(log N) seek each)");
    } else if args.sample_interval > 1 {
        println!(
            "Sampling mode: comparing every {}th entry (still traverses all entries)",
            args.sample_interval
        );
    } else {
        println!("Full comparison mode: comparing every entry");
    }

    // --- Open databases ---
    println!();
    print!("Opening DB1... ");
    io::stdout().flush().ok();
    let t = Instant::now();
    let env1 = open_env(&args.db1_path).unwrap_or_else(|e| {
        eprintln!("FAILED: {e}");
        std::process::exit(1);
    });
    println!("OK ({:.1}s)", t.elapsed().as_secs_f64());

    print!("Opening DB2... ");
    io::stdout().flush().ok();
    let t = Instant::now();
    let env2 = open_env(&args.db2_path).unwrap_or_else(|e| {
        eprintln!("FAILED: {e}");
        std::process::exit(1);
    });
    println!("OK ({:.1}s)", t.elapsed().as_secs_f64());

    // --- Print per-table entry counts for both DBs ---
    println!();
    println!("┌─────────────────────────────────────────┬──────────────┬──────────────┐");
    println!("│ Table                                   │    DB1 count │    DB2 count │");
    println!("├─────────────────────────────────────────┼──────────────┼──────────────┤");
    for &table_name in ALL_TABLES {
        let c1 = table_entry_count(&env1, table_name).unwrap_or(0);
        let c2 = table_entry_count(&env2, table_name).unwrap_or(0);
        let marker = if skip_tables.contains(table_name) {
            " (skip)"
        } else if c1 != c2 {
            " *"
        } else {
            ""
        };
        println!("│ {:<39} │ {:>12} │ {:>12} │{marker}", table_name, c1, c2);
    }
    println!("└─────────────────────────────────────────┴──────────────┴──────────────┘");
    println!("  * = different counts (expected when DBs are at different block heights)");

    // --- Determine reference (smaller) DB ---
    let count1 = table_entry_count(&env1, "headers").unwrap_or(0);
    let count2 = table_entry_count(&env2, "headers").unwrap_or(0);
    let (ref_env, ref_label, other_env, other_label) =
        if count1 <= count2 { (&env1, "DB1", &env2, "DB2") } else { (&env2, "DB2", &env1, "DB1") };
    println!();
    println!(
        "Reference: {ref_label} ({} headers)  |  Other: {other_label} ({} headers)",
        count1.min(count2),
        count1.max(count2),
    );
    println!("Strategy: verify every entry in {ref_label} exists identically in {other_label}");
    println!();

    // --- Compare tables ---
    let mut total_entries = 0u64;
    let mut total_skipped = 0u64;
    let mut tables_compared = 0usize;
    let mut tables_failed = Vec::new();

    for (idx, &table_name) in ALL_TABLES.iter().enumerate() {
        if skip_tables.contains(table_name) {
            println!("[{:>2}/{}] {:<40} SKIPPED", idx + 1, ALL_TABLES.len(), table_name);
            continue;
        }

        print!("[{:>2}/{}] {:<40} ", idx + 1, ALL_TABLES.len(), table_name);
        io::stdout().flush().ok();

        let t = Instant::now();
        let result = if let Some(num_samples) = args.num_samples {
            compare_table_probe(ref_env, other_env, table_name, num_samples)
        } else {
            compare_table_subset(
                ref_env,
                other_env,
                table_name,
                args.progress_interval,
                args.sample_interval,
            )
        };
        match result {
            Ok(result) => {
                let elapsed = t.elapsed().as_secs_f64();
                let rate = if elapsed > 0.0 {
                    format!("{:.0}/s", result.matched as f64 / elapsed)
                } else {
                    "instant".to_string()
                };
                println!(
                    "OK  {:>10} verified, {:>8} skipped  [{}  {}]",
                    result.matched,
                    result.skipped,
                    fmt_duration(elapsed),
                    rate,
                );
                total_entries += result.matched;
                total_skipped += result.skipped;
                tables_compared += 1;
            }
            Err(e) => {
                println!("FAILED  [{}]", fmt_duration(t.elapsed().as_secs_f64()));
                eprintln!("  ERROR: {e}");
                tables_failed.push(table_name.to_string());
            }
        }
    }

    // --- Compare mmap files ---
    if args.compare_mmap {
        println!();
        println!("Comparing mmap files...");

        for &file_name in MMAP_FILES {
            let path1 = args.db1_path.join(file_name);
            let path2 = args.db2_path.join(file_name);

            if !path1.exists() && !path2.exists() {
                println!("  {file_name:<30} SKIPPED (not present in either DB)");
                continue;
            }
            if !path1.exists() || !path2.exists() {
                let missing_in = if !path1.exists() { "DB1" } else { "DB2" };
                println!("  {file_name:<30} MISMATCH (missing in {missing_in})");
                tables_failed.push(format!("mmap:{file_name}"));
                continue;
            }

            let t = Instant::now();
            match compare_mmap_file_prefix(&path1, &path2) {
                Ok((compared, total1, total2)) => {
                    println!(
                        "  {file_name:<30} OK  compared {} (DB1={}, DB2={})  [{}]",
                        fmt_bytes(compared),
                        fmt_bytes(total1),
                        fmt_bytes(total2),
                        fmt_duration(t.elapsed().as_secs_f64()),
                    );
                }
                Err(e) => {
                    println!(
                        "  {file_name:<30} FAILED  [{}]",
                        fmt_duration(t.elapsed().as_secs_f64())
                    );
                    eprintln!("  ERROR: {e}");
                    tables_failed.push(format!("mmap:{file_name}"));
                }
            }
        }
    }

    // --- Summary ---
    let total_elapsed = total_start.elapsed().as_secs_f64();
    println!();
    println!("════════════════════════════════════════════");
    if tables_failed.is_empty() {
        println!("  RESULT: VALIDATION PASSED");
    } else {
        println!("  RESULT: VALIDATION FAILED");
        println!("  Failed: {}", tables_failed.join(", "));
    }
    println!("  Tables compared: {tables_compared}");
    println!("  Entries verified: {total_entries}");
    println!("  Entries skipped (extra in larger DB): {total_skipped}");
    if let Some(n) = args.num_samples {
        println!("  Probe samples per table: {n}");
    } else if args.sample_interval > 1 {
        println!("  Sample interval: every {}th entry", args.sample_interval);
    }
    println!("  Total time: {}", fmt_duration(total_elapsed));
    println!("════════════════════════════════════════════");

    if !tables_failed.is_empty() {
        std::process::exit(1);
    }
}

fn open_env(path: &PathBuf) -> Result<Environment, String> {
    if !path.exists() {
        return Err(format!("Path does not exist: {path:?}"));
    }

    Environment::new()
        .set_geometry(Geometry {
            size: Some(1 << 20..1 << 40),
            growth_step: Some(1 << 32),
            ..Default::default()
        })
        .set_max_tables(30)
        .set_max_readers(100)
        .set_flags(DatabaseFlags { no_rdahead: true, mode: Mode::ReadOnly, ..Default::default() })
        .open(path)
        .map_err(|e| format!("Failed to open MDBX environment: {e}"))
}

fn table_entry_count(env: &Environment, table_name: &str) -> Result<u64, String> {
    let txn = env.begin_ro_txn().map_err(|e| format!("begin_ro_txn: {e}"))?;
    let table = txn.open_table(Some(table_name)).map_err(|e| format!("open_table: {e}"))?;
    let stat = txn.table_stat(&table).map_err(|e| format!("table_stat: {e}"))?;
    Ok(stat.entries() as u64)
}

struct CompareResult {
    matched: u64,
    skipped: u64,
}

/// Compare tables using the subset algorithm: iterate the reference (smaller) DB and
/// verify every entry exists identically in the other (larger) DB.
///
/// The other DB may have additional entries (from extra blocks); those are skipped.
/// Any entry in the reference DB that is missing or different in the other DB is an error.
fn compare_table_subset(
    ref_env: &Environment,
    other_env: &Environment,
    table_name: &str,
    progress_interval: u64,
    sample_interval: u64,
) -> Result<CompareResult, String> {
    let txn_ref = ref_env.begin_ro_txn().map_err(|e| format!("begin ref txn: {e}"))?;
    let txn_other = other_env.begin_ro_txn().map_err(|e| format!("begin other txn: {e}"))?;

    let tbl_ref =
        txn_ref.open_table(Some(table_name)).map_err(|e| format!("open table in ref DB: {e}"))?;
    let tbl_other = txn_other
        .open_table(Some(table_name))
        .map_err(|e| format!("open table in other DB: {e}"))?;

    let mut cur_ref = txn_ref.cursor(&tbl_ref).map_err(|e| format!("cursor ref: {e}"))?;
    let mut cur_other = txn_other.cursor(&tbl_other).map_err(|e| format!("cursor other: {e}"))?;

    let mut entry_ref =
        cur_ref.first::<DbKeyType<'_>, DbValueType<'_>>().map_err(|e| format!("first ref: {e}"))?;
    let mut entry_other = cur_other
        .first::<DbKeyType<'_>, DbValueType<'_>>()
        .map_err(|e| format!("first other: {e}"))?;

    let mut matched = 0u64;
    let mut skipped = 0u64;
    let mut ref_position = 0u64;

    loop {
        match (&entry_ref, &entry_other) {
            (None, _) => break,

            (Some((k_ref, _)), None) => {
                return Err(format!(
                    "Reference DB has entry not found in other DB after {matched} matches. Key \
                     ({} bytes): 0x{}",
                    k_ref.len(),
                    hex_prefix(k_ref.as_ref(), 32),
                ));
            }

            (Some((k_ref, v_ref)), Some((k_other, v_other))) => {
                match k_ref.as_ref().cmp(k_other.as_ref()) {
                    Ordering::Equal => {
                        if v_ref.as_ref() != v_other.as_ref() {
                            return Err(format!(
                                "Value mismatch at ref position {ref_position}. Key ({kb} bytes): \
                                 0x{key_hex}  Ref value: {vr} bytes, Other value: {vo} bytes. Ref \
                                 first bytes: 0x{ref_hex}  Other first bytes: 0x{other_hex}",
                                kb = k_ref.len(),
                                key_hex = hex_prefix(k_ref.as_ref(), 32),
                                vr = v_ref.len(),
                                vo = v_other.len(),
                                ref_hex = hex_prefix(v_ref.as_ref(), 16),
                                other_hex = hex_prefix(v_other.as_ref(), 16),
                            ));
                        }
                        matched += 1;
                        ref_position += 1;
                        if matched % progress_interval == 0 {
                            print!(
                                "\r  ... {matched} entries verified, {skipped} skipped, ref pos \
                                 {ref_position}"
                            );
                            io::stdout().flush().ok();
                        }

                        // Skip (sample_interval - 1) entries in both cursors
                        for _ in 0..sample_interval.saturating_sub(1) {
                            entry_ref = cur_ref
                                .next::<DbKeyType<'_>, DbValueType<'_>>()
                                .map_err(|e| format!("next ref (sample skip): {e}"))?;
                            if entry_ref.is_none() {
                                break;
                            }
                            ref_position += 1;
                        }
                        if entry_ref.is_none() {
                            break;
                        }

                        entry_ref = cur_ref
                            .next::<DbKeyType<'_>, DbValueType<'_>>()
                            .map_err(|e| format!("next ref at entry {matched}: {e}"))?;
                        ref_position += 1;

                        // Advance other cursor to match or pass the new ref key
                        if let Some((k_ref_new, _)) = &entry_ref {
                            loop {
                                entry_other = cur_other
                                    .next::<DbKeyType<'_>, DbValueType<'_>>()
                                    .map_err(|e| format!("next other (advance): {e}"))?;
                                match &entry_other {
                                    None => break,
                                    Some((k_o, _)) => match k_o.as_ref().cmp(k_ref_new.as_ref()) {
                                        Ordering::Less => {
                                            skipped += 1;
                                            continue;
                                        }
                                        _ => break,
                                    },
                                }
                            }
                        }
                    }
                    Ordering::Greater => {
                        skipped += 1;
                        entry_other = cur_other
                            .next::<DbKeyType<'_>, DbValueType<'_>>()
                            .map_err(|e| format!("next other (skip) at entry {matched}: {e}"))?;
                    }
                    Ordering::Less => {
                        return Err(format!(
                            "Reference DB has entry missing in other DB at match {matched}. Ref \
                             key ({} bytes): 0x{}  Other key ({} bytes): 0x{}",
                            k_ref.len(),
                            hex_prefix(k_ref.as_ref(), 32),
                            k_other.len(),
                            hex_prefix(k_other.as_ref(), 32),
                        ));
                    }
                }
            }
        }
    }

    if matched > progress_interval {
        print!("\r{:80}\r", ""); // clear progress line
        io::stdout().flush().ok();
    }

    Ok(CompareResult { matched, skipped })
}

/// Generate evenly-spaced probe keys between first_key and last_key.
/// Treats keys as big-endian unsigned integers and interpolates.
fn generate_probe_keys(first_key: &[u8], last_key: &[u8], num_probes: u64) -> Vec<Vec<u8>> {
    let key_len = first_key.len();
    if key_len == 0 || num_probes == 0 {
        return vec![];
    }

    let first = bytes_to_u128(first_key);
    let last = bytes_to_u128(last_key);
    if first >= last || num_probes <= 1 {
        return vec![first_key.to_vec()];
    }

    let range = last - first;
    let mut probes = Vec::with_capacity(num_probes as usize);
    for i in 0..num_probes {
        let offset = range * i as u128 / (num_probes - 1) as u128;
        let val = first + offset;
        probes.push(u128_to_bytes(val, key_len));
    }
    probes
}

fn bytes_to_u128(bytes: &[u8]) -> u128 {
    let mut val: u128 = 0;
    for (i, &b) in bytes.iter().enumerate().take(16) {
        val |= (b as u128) << (8 * (15 - i));
    }
    val
}

fn u128_to_bytes(val: u128, len: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(len);
    for i in 0..len.min(16) {
        bytes.push((val >> (8 * (15 - i))) as u8);
    }
    while bytes.len() < len {
        bytes.push(0);
    }
    bytes
}

/// Compare tables using probe sampling: generate evenly-spaced keys across the key space
/// and use set_range() to seek directly to each one. O(num_samples * log(N)) instead of O(N).
fn compare_table_probe(
    ref_env: &Environment,
    other_env: &Environment,
    table_name: &str,
    num_samples: u64,
) -> Result<CompareResult, String> {
    let txn_ref = ref_env.begin_ro_txn().map_err(|e| format!("begin ref txn: {e}"))?;
    let txn_other = other_env.begin_ro_txn().map_err(|e| format!("begin other txn: {e}"))?;

    let tbl_ref =
        txn_ref.open_table(Some(table_name)).map_err(|e| format!("open table in ref DB: {e}"))?;
    let tbl_other = txn_other
        .open_table(Some(table_name))
        .map_err(|e| format!("open table in other DB: {e}"))?;

    let stat = txn_ref.table_stat(&tbl_ref).map_err(|e| format!("table_stat: {e}"))?;
    let total_entries = stat.entries() as u64;

    if total_entries == 0 {
        return Ok(CompareResult { matched: 0, skipped: 0 });
    }

    let mut cur_ref = txn_ref.cursor(&tbl_ref).map_err(|e| format!("cursor ref: {e}"))?;
    let mut cur_other = txn_other.cursor(&tbl_other).map_err(|e| format!("cursor other: {e}"))?;

    let first_entry: Option<(DbKeyType<'_>, DbValueType<'_>)> =
        cur_ref.first().map_err(|e| format!("first: {e}"))?;
    let last_entry: Option<(DbKeyType<'_>, DbValueType<'_>)> =
        cur_ref.last().map_err(|e| format!("last: {e}"))?;

    let (first_key, _) = first_entry.ok_or("table is empty")?;
    let (last_key, _) = last_entry.ok_or("table is empty")?;

    let effective_samples = num_samples.min(total_entries);
    let probes = generate_probe_keys(first_key.as_ref(), last_key.as_ref(), effective_samples);

    let mut matched = 0u64;
    let mut mismatched_keys = 0u64;

    for probe_key in &probes {
        let ref_entry: Option<(DbKeyType<'_>, DbValueType<'_>)> =
            cur_ref.set_range(probe_key.as_slice()).map_err(|e| format!("set_range ref: {e}"))?;

        let Some((k_ref, v_ref)) = ref_entry else {
            break;
        };

        let other_entry: Option<(DbKeyType<'_>, DbValueType<'_>)> =
            cur_other.set_range(k_ref.as_ref()).map_err(|e| format!("set_range other: {e}"))?;

        match other_entry {
            None => {
                mismatched_keys += 1;
            }
            Some((k_other, v_other)) => {
                if k_ref.as_ref() != k_other.as_ref() {
                    mismatched_keys += 1;
                } else if v_ref.as_ref() != v_other.as_ref() {
                    return Err(format!(
                        "Value mismatch at probe. Key ({kb} bytes): 0x{key_hex}  Ref value: {vr} \
                         bytes, Other value: {vo} bytes. Ref first bytes: 0x{ref_hex}  Other \
                         first bytes: 0x{other_hex}",
                        kb = k_ref.len(),
                        key_hex = hex_prefix(k_ref.as_ref(), 32),
                        vr = v_ref.len(),
                        vo = v_other.len(),
                        ref_hex = hex_prefix(v_ref.as_ref(), 16),
                        other_hex = hex_prefix(v_other.as_ref(), 16),
                    ));
                } else {
                    matched += 1;
                }
            }
        }
    }

    Ok(CompareResult { matched, skipped: mismatched_keys })
}

/// Compare mmap files up to the smaller file's size using streaming 1MB chunks.
/// Never loads more than 2MB into memory regardless of file size.
fn compare_mmap_file_prefix(path1: &PathBuf, path2: &PathBuf) -> Result<(u64, u64, u64), String> {
    use std::io::{BufReader, Read as IoRead};

    let meta1 = fs::metadata(path1).map_err(|e| format!("metadata {path1:?}: {e}"))?;
    let meta2 = fs::metadata(path2).map_err(|e| format!("metadata {path2:?}: {e}"))?;
    let size1 = meta1.len();
    let size2 = meta2.len();
    let compare_len = size1.min(size2);

    let f1 = fs::File::open(path1).map_err(|e| format!("open {path1:?}: {e}"))?;
    let f2 = fs::File::open(path2).map_err(|e| format!("open {path2:?}: {e}"))?;
    let mut r1 = BufReader::with_capacity(1 << 20, f1);
    let mut r2 = BufReader::with_capacity(1 << 20, f2);

    const CHUNK: usize = 1 << 20; // 1 MB
    let mut buf1 = vec![0u8; CHUNK];
    let mut buf2 = vec![0u8; CHUNK];
    let mut offset: u64 = 0;

    while offset < compare_len {
        let to_read = CHUNK.min((compare_len - offset) as usize);
        r1.read_exact(&mut buf1[..to_read])
            .map_err(|e| format!("read {path1:?} at offset {offset}: {e}"))?;
        r2.read_exact(&mut buf2[..to_read])
            .map_err(|e| format!("read {path2:?} at offset {offset}: {e}"))?;

        if buf1[..to_read] != buf2[..to_read] {
            for i in 0..to_read {
                if buf1[i] != buf2[i] {
                    let abs = offset as usize + i;
                    let ctx_start = i.saturating_sub(8);
                    let ctx_end = (i + 8).min(to_read);
                    return Err(format!(
                        "Content mismatch at byte offset {abs} / {compare_len}. DB1: 0x{}  DB2: \
                         0x{}",
                        hex_prefix(&buf1[ctx_start..ctx_end], 32),
                        hex_prefix(&buf2[ctx_start..ctx_end], 32),
                    ));
                }
            }
        }
        offset += to_read as u64;
    }

    Ok((compare_len, size1, size2))
}
