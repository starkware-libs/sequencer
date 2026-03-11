use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use apollo_infra_utils::compile_time_cargo_manifest_dir;
use apollo_infra_utils::path::project_path;
use digest::Digest;
use sha2::Sha256;

use crate::cairo_compile::{
    cairo1_compile,
    lock_path_for,
    verify_cairo1_package,
    with_file_lock,
    CompilationArtifacts,
    LibfuncArg,
};
use crate::contracts::FeatureContract;

static CACHE_DIR: LazyLock<PathBuf> =
    LazyLock::new(|| project_path().unwrap().join("target/blockifier_test_artifacts"));

/// Returns the cached CASM path for a Cairo 1 feature contract.
pub fn cached_compiled_path(contract: &FeatureContract) -> PathBuf {
    CACHE_DIR.join(format!("cairo1/compiled/{}.casm.json", contract.get_non_erc20_base_name()))
}

/// Returns the cached Sierra path for a Cairo 1 feature contract.
pub fn cached_sierra_path(contract: &FeatureContract) -> PathBuf {
    CACHE_DIR.join(format!("cairo1/sierra/{}.sierra.json", contract.get_non_erc20_base_name()))
}

/// Single cache key file per contract — both CASM and Sierra are produced by the same
/// compilation, so one key covers both.
fn cache_key_path(contract: &FeatureContract) -> PathBuf {
    CACHE_DIR.join(format!("{}.hash", contract.get_non_erc20_base_name()))
}

/// Computes a cache key from the source content, compiler version, and libfunc argument.
fn compute_cache_key(
    source_path: &Path,
    compiler_version: &str,
    crate_root: &Path,
    libfunc_arg: &LibfuncArg,
) -> String {
    let source_content = fs::read_to_string(source_path)
        .unwrap_or_else(|e| panic!("Cannot read {source_path:?}: {e}"));

    let mut hasher = Sha256::new();
    hasher.update(source_content.as_bytes());
    hasher.update(b"\x00");
    hasher.update(compiler_version.as_bytes());
    hasher.update(b"\x00");
    match libfunc_arg {
        LibfuncArg::ListFile(file) => {
            let abs = crate_root.join(file);
            let content = fs::read_to_string(&abs)
                .unwrap_or_else(|e| panic!("Cannot read libfunc file {abs:?}: {e}"));
            hasher.update(content.as_bytes());
        }
        LibfuncArg::ListName(name) => {
            hasher.update(b"list-name:");
            hasher.update(name.as_bytes());
        }
    }
    format!("{:x}", hasher.finalize())
}

fn is_cache_fresh(contract: &FeatureContract, expected_hash: &str) -> bool {
    fs::read_to_string(cache_key_path(contract))
        .is_ok_and(|stored_hash| stored_hash.trim() == expected_hash)
}

fn write_artifact(path: &Path, content: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .unwrap_or_else(|e| panic!("Failed to create directory {parent:?}: {e}"));
    }
    fs::write(path, content).unwrap_or_else(|e| panic!("Failed to write {path:?}: {e}"));
}

/// Per-contract lock file for serializing compilation (distinct from the per-version compiler
/// download lock in `cairo_compile::verify_cairo1_package`).
fn lock_file_path(contract: &FeatureContract) -> PathBuf {
    lock_path_for(&CACHE_DIR.join(contract.get_non_erc20_base_name()))
}

/// Ensures a Cairo 1 feature contract is compiled and cached. Returns immediately if the cache
/// is fresh. Uses a per-contract file lock so that concurrent callers (threads or processes)
/// never compile the same contract redundantly.
pub fn ensure_cairo1_compiled(contract: &FeatureContract) {
    assert!(
        !matches!(contract, FeatureContract::ERC20(_)),
        "ERC20 uses committed artifacts, not the compilation cache."
    );
    assert!(
        !contract.cairo_version().is_cairo0(),
        "Cairo 0 contracts use committed artifacts, not the compilation cache."
    );

    let casm_path = cached_compiled_path(contract);
    let sierra_path = cached_sierra_path(contract);
    let version = contract.fixed_version();
    let crate_root = PathBuf::from(compile_time_cargo_manifest_dir!());
    let source_path = crate_root.join(contract.get_source_path());
    let libfunc_arg = contract.libfunc_arg();
    let abs_libfunc_arg = match &libfunc_arg {
        LibfuncArg::ListFile(file) => {
            LibfuncArg::ListFile(crate_root.join(file).to_string_lossy().to_string())
        }
        LibfuncArg::ListName(name) => LibfuncArg::ListName(name.clone()),
    };

    let cache_key = compute_cache_key(&source_path, &version, &crate_root, &libfunc_arg);

    let is_fresh = || is_cache_fresh(contract, &cache_key);

    with_file_lock(&lock_file_path(contract), is_fresh, || {
        let start = std::time::Instant::now();
        eprintln!(
            "[compile_cache] Cache miss for {contract:?} (compiler v{version}), compiling from \
             source..."
        );
        verify_cairo1_package(&version);

        let CompilationArtifacts::Cairo1 { casm, sierra } =
            cairo1_compile(source_path.to_string_lossy().to_string(), version, abs_libfunc_arg)
        else {
            unreachable!("cairo1_compile always returns Cairo1 variant");
        };

        write_artifact(&casm_path, &casm);
        write_artifact(&sierra_path, &sierra);
        // Written last: acts as a commit marker for the compilation above.
        write_artifact(&cache_key_path(contract), cache_key.as_bytes());

        eprintln!(
            "[compile_cache] Compiled and cached {contract:?} in {:.1}s",
            start.elapsed().as_secs_f64()
        );
    });
}
