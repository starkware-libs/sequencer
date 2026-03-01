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
        match compare_table_subset(ref_env, other_env, table_name, args.progress_interval) {
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
                                "Value mismatch at entry {matched}. \
                                 Key ({kb} bytes): 0x{key_hex}  \
                                 Ref value: {vr} bytes, Other value: {vo} bytes. \
                                 Ref first bytes: 0x{ref_hex}  \
                                 Other first bytes: 0x{other_hex}",
                                kb = k_ref.len(),
                                key_hex = hex_prefix(k_ref.as_ref(), 32),
                                vr = v_ref.len(),
                                vo = v_other.len(),
                                ref_hex = hex_prefix(v_ref.as_ref(), 16),
                                other_hex = hex_prefix(v_other.as_ref(), 16),
                            ));
                        }
                        matched += 1;
                        if matched % progress_interval == 0 {
                            print!("\r  ... {matched} entries verified, {skipped} skipped");
                            io::stdout().flush().ok();
                        }

                        entry_ref = cur_ref
                            .next::<DbKeyType<'_>, DbValueType<'_>>()
                            .map_err(|e| format!("next ref at entry {matched}: {e}"))?;
                        entry_other = cur_other
                            .next::<DbKeyType<'_>, DbValueType<'_>>()
                            .map_err(|e| format!("next other at entry {matched}: {e}"))?;
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

/// Compare mmap files up to the smaller file's size. The larger file may have extra
/// data appended from additional blocks, which is expected and ignored.
fn compare_mmap_file_prefix(path1: &PathBuf, path2: &PathBuf) -> Result<(u64, u64, u64), String> {
    let data1 = fs::read(path1).map_err(|e| format!("read {path1:?}: {e}"))?;
    let data2 = fs::read(path2).map_err(|e| format!("read {path2:?}: {e}"))?;

    let compare_len = data1.len().min(data2.len());

    for i in 0..compare_len {
        if data1[i] != data2[i] {
            let context_start = i.saturating_sub(8);
            let context_end = (i + 8).min(compare_len);
            return Err(format!(
                "Content mismatch at byte offset {i} / {compare_len}. \
                 DB1[{context_start}..{context_end}]: 0x{}  DB2[{context_start}..{context_end}]: \
                 0x{}",
                hex_prefix(&data1[context_start..context_end], 32),
                hex_prefix(&data2[context_start..context_end], 32),
            ));
        }
    }

    Ok((compare_len as u64, data1.len() as u64, data2.len() as u64))
}
