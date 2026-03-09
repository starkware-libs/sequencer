use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use apollo_infra_utils::compile_time_cargo_manifest_dir;
use apollo_infra_utils::path::project_path;
use digest::Digest;
use sha2::Sha256;

use crate::cairo_compile::{cairo1_compile, verify_cairo1_package, CompilationArtifacts};
use crate::contracts::FeatureContract;

static CACHE_DIR: LazyLock<PathBuf> =
    LazyLock::new(|| project_path().unwrap().join("target/blockifier_test_artifacts"));

/// Returns the root directory for cached compilation artifacts.
pub fn cache_dir() -> &'static Path {
    &CACHE_DIR
}

/// Returns the cached CASM path for a Cairo 1 feature contract.
pub fn cached_compiled_path(contract: &FeatureContract) -> PathBuf {
    cache_dir().join(format!("cairo1/compiled/{}.casm.json", contract.get_non_erc20_base_name()))
}

/// Returns the cached Sierra path for a Cairo 1 feature contract.
pub fn cached_sierra_path(contract: &FeatureContract) -> PathBuf {
    cache_dir().join(format!("cairo1/sierra/{}.sierra.json", contract.get_non_erc20_base_name()))
}

fn hash_sidecar_path(artifact_path: &Path) -> PathBuf {
    artifact_path.with_extension(format!(
        "{}.hash",
        artifact_path.extension().unwrap_or_default().to_string_lossy()
    ))
}

/// Computes a cache key from the source content, compiler version, and libfunc list file content.
fn compute_cache_key(source_path: &str, compiler_version: &str, libfunc_file: &str) -> String {
    let source_content = fs::read_to_string(source_path)
        .unwrap_or_else(|e| panic!("Cannot read {source_path}: {e}"));
    let libfunc_content = fs::read_to_string(libfunc_file)
        .unwrap_or_else(|e| panic!("Cannot read libfunc file {libfunc_file}: {e}"));

    let mut hasher = Sha256::new();
    hasher.update(source_content.as_bytes());
    hasher.update(b"\x00");
    hasher.update(compiler_version.as_bytes());
    hasher.update(b"\x00");
    hasher.update(libfunc_content.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn is_cache_fresh(artifact_path: &Path, expected_hash: &str) -> bool {
    let hash_path = hash_sidecar_path(artifact_path);
    match fs::read_to_string(&hash_path) {
        Ok(stored_hash) => stored_hash.trim() == expected_hash && artifact_path.exists(),
        Err(_) => false,
    }
}

/// Atomically writes content to `path` via a temp file rename.
fn atomic_write(path: &Path, content: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|e| {
            panic!("Failed to create directory {parent:?}: {e}");
        });
    }
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, content)
        .unwrap_or_else(|e| panic!("Failed to write temp file {tmp_path:?}: {e}"));
    fs::rename(&tmp_path, path)
        .unwrap_or_else(|e| panic!("Failed to rename {tmp_path:?} -> {path:?}: {e}"));
}

/// Resolves a path relative to the blockifier_test_utils crate root to an absolute path.
fn resolve_crate_relative(relative: &str) -> String {
    PathBuf::from(compile_time_cargo_manifest_dir!()).join(relative).to_string_lossy().to_string()
}

/// Ensures a Cairo 1 feature contract is compiled and cached. Returns immediately if the cache
/// is fresh. Thread-safe via atomic file writes (concurrent compilations of the same contract
/// produce identical output).
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
    let source_path = resolve_crate_relative(&contract.get_source_path());
    let libfunc_arg = contract.libfunc_arg();
    let libfunc_file = resolve_crate_relative(libfunc_arg.file_path());

    let cache_key = compute_cache_key(&source_path, &version, &libfunc_file);

    if is_cache_fresh(&casm_path, &cache_key) && is_cache_fresh(&sierra_path, &cache_key) {
        return;
    }

    let start = std::time::Instant::now();
    eprintln!(
        "[compile_cache] Cache miss for {contract:?} (compiler v{version}), compiling from \
         source..."
    );
    verify_cairo1_package(&version);

    let CompilationArtifacts::Cairo1 { casm, sierra } =
        cairo1_compile(source_path, version, libfunc_arg)
    else {
        unreachable!("cairo1_compile always returns Cairo1 variant");
    };

    atomic_write(&casm_path, &casm);
    atomic_write(&hash_sidecar_path(&casm_path), cache_key.as_bytes());
    atomic_write(&sierra_path, &sierra);
    atomic_write(&hash_sidecar_path(&sierra_path), cache_key.as_bytes());

    eprintln!(
        "[compile_cache] Compiled and cached {contract:?} in {:.1}s",
        start.elapsed().as_secs_f64()
    );
}
