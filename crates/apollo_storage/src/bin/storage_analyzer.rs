#![cfg(feature = "storage_analyzer")]

//! Binary for analyzing Apollo storage databases to measure optimization opportunities.
//!
//! Opens a populated MDBX database in read-only mode and reports per-table sizes, flat state
//! savings estimates, varint encoding savings, mmap compression ratios, and more.

use std::path::{Path, PathBuf};

use apollo_storage::db::DbConfig;
use apollo_storage::mmap_file::MmapFileConfig;
use apollo_storage::storage_analysis::{run_analysis, AnalysisConfig};
use apollo_storage::{open_storage_read_only, StorageConfig, StorageScope};
use clap::Parser;

#[derive(Parser)]
#[command(
    name = "storage_analyzer",
    about = "Analyze Apollo MDBX storage for optimization opportunities"
)]
struct Args {
    /// Path to the MDBX chain directory (e.g., /data/batcher/SN_MAIN).
    #[arg(long)]
    db_path: PathBuf,

    /// Storage scope: "state_only" or "full_archive".
    #[arg(long, default_value = "full_archive")]
    scope: String,

    /// Second DB path for duplication comparison.
    #[arg(long)]
    compare_db_path: Option<PathBuf>,

    /// Number of mmap entries to sample for compression analysis.
    #[arg(long, default_value = "1000")]
    mmap_sample_count: usize,

    /// Skip full table iteration (M2/M3). Only run quick measurements.
    #[arg(long)]
    skip_full_iteration: bool,

    /// Output JSON only, no human-readable summary.
    #[arg(long)]
    json: bool,
}

fn parse_db_path(full_path: &Path) -> (PathBuf, String) {
    let chain_id = full_path
        .file_name()
        .expect("db_path must have a final component (e.g., SN_MAIN)")
        .to_string_lossy()
        .to_string();
    let path_prefix =
        full_path.parent().expect("db_path must have a parent directory").to_path_buf();
    (path_prefix, chain_id)
}

fn main() {
    let args = Args::parse();

    let scope = match args.scope.as_str() {
        "state_only" => StorageScope::StateOnly,
        "full_archive" => StorageScope::FullArchive,
        other => {
            eprintln!("Unknown scope '{other}'. Use 'state_only' or 'full_archive'.");
            std::process::exit(1);
        }
    };

    let (path_prefix, chain_id_str) = parse_db_path(&args.db_path);
    let chain_id = starknet_api::core::ChainId::Other(chain_id_str);

    let storage_config = StorageConfig {
        db_config: DbConfig {
            path_prefix,
            chain_id,
            enforce_file_exists: true,
            min_size: 1 << 20,
            max_size: 1 << 40,
            growth_step: 1 << 32,
            max_readers: 1 << 13,
        },
        mmap_file_config: MmapFileConfig::default(),
        scope,
    };

    let reader = open_storage_read_only(storage_config).unwrap_or_else(|err| {
        eprintln!("Failed to open storage at {}: {err}", args.db_path.display());
        std::process::exit(1);
    });

    let config = AnalysisConfig {
        mmap_sample_count: args.mmap_sample_count,
        compare_db_path: args.compare_db_path,
        skip_full_iteration: args.skip_full_iteration,
    };

    let report = run_analysis(&reader, &config).unwrap_or_else(|err| {
        eprintln!("Analysis failed: {err}");
        std::process::exit(1);
    });

    if !args.json {
        eprint!("{}", report.human_readable_summary());
    }
    println!("{}", serde_json::to_string_pretty(&report).expect("Failed to serialize report"));
}
