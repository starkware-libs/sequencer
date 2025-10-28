use std::path::PathBuf;

use rstest::fixture;

/// Returns the bench_tools crate directory.
#[fixture]
pub fn bench_tools_crate_dir() -> PathBuf {
    std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap())
}
