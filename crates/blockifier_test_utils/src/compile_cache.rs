use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use apollo_infra_utils::compile_time_cargo_manifest_dir;
use apollo_infra_utils::path::project_path;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use digest::Digest;
use sha2::Sha256;
use starknet_api::contract_class::compiled_class_hash::{HashVersion, HashableCompiledClass};
use starknet_api::core::CompiledClassHash;
use starknet_api::felt;

use crate::cairo_compile::{
    cairo1_compile,
    verify_cairo1_package,
    with_file_lock,
    CompilationArtifacts,
    LibfuncArg,
};
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

/// Returns the base name used for cache files. Works for all contracts including ERC20.
fn cache_base_name(contract: &FeatureContract) -> &str {
    if matches!(contract, FeatureContract::ERC20(_)) {
        "erc20"
    } else {
        contract.get_non_erc20_base_name()
    }
}

/// Returns the cached compiled class hash path for a given contract and hash version.
fn cached_compiled_class_hash_path(
    contract: &FeatureContract,
    hash_version: &HashVersion,
) -> PathBuf {
    let suffix = match hash_version {
        HashVersion::V1 => "v1",
        HashVersion::V2 => "v2",
    };
    cache_dir().join(format!("cairo1/compiled_class_hashes/{}.{suffix}", cache_base_name(contract)))
}

/// Computes compiled class hashes (V1 and V2) from raw CASM JSON and writes them to cache files.
fn compute_and_write_compiled_class_hashes(contract: &FeatureContract, casm_json: &[u8]) {
    let casm: CasmContractClass = serde_json::from_slice(casm_json)
        .unwrap_or_else(|e| panic!("Failed to deserialize CASM for {contract:?}: {e}"));
    for version in [HashVersion::V1, HashVersion::V2] {
        let hash_hex = format!("0x{:x}", casm.hash(&version).0);
        atomic_write(&cached_compiled_class_hash_path(contract, &version), hash_hex.as_bytes());
    }
}

/// Reads a cached compiled class hash for a given contract and hash version.
pub fn read_cached_compiled_class_hash(
    contract: &FeatureContract,
    hash_version: &HashVersion,
) -> CompiledClassHash {
    let path = cached_compiled_class_hash_path(contract, hash_version);
    let hex = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read cached class hash from {path:?}: {e}"));
    CompiledClassHash(felt!(hex.trim()))
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

/// Atomically writes content to `path` via a unique temp file + rename.
fn atomic_write(path: &Path, content: &[u8]) {
    let parent = path.parent().expect("artifact path must have a parent directory");
    fs::create_dir_all(parent)
        .unwrap_or_else(|e| panic!("Failed to create directory {parent:?}: {e}"));

    // Each call gets its own temp file, so concurrent writers never collide.
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .unwrap_or_else(|e| panic!("Failed to create temp file in {parent:?}: {e}"));
    std::io::Write::write_all(&mut tmp, content)
        .unwrap_or_else(|e| panic!("Failed to write temp file {:?}: {e}", tmp.path()));
    tmp.persist(path).unwrap_or_else(|e| {
        panic!("Failed to persist temp file to {path:?}: {e}");
    });
}

/// Resolves a path relative to the blockifier_test_utils crate root to an absolute path.
fn resolve_crate_relative(relative: &str) -> String {
    PathBuf::from(compile_time_cargo_manifest_dir!()).join(relative).to_string_lossy().to_string()
}

/// Returns the path of the lock file used to serialize compilation of a given contract.
fn lock_file_path(contract: &FeatureContract) -> PathBuf {
    cache_dir().join(format!("{}.lock", contract.get_non_erc20_base_name()))
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
    let source_path = resolve_crate_relative(&contract.get_source_path());
    let libfunc_arg =
        LibfuncArg::ListFile(resolve_crate_relative(contract.libfunc_arg().file_path()));

    let cache_key = compute_cache_key(&source_path, &version, libfunc_arg.file_path());

    let compiled_class_hash_v1_path = cached_compiled_class_hash_path(contract, &HashVersion::V1);
    let compiled_class_hash_v2_path = cached_compiled_class_hash_path(contract, &HashVersion::V2);

    let is_fresh = || {
        is_cache_fresh(&casm_path, &cache_key)
            && is_cache_fresh(&sierra_path, &cache_key)
            && compiled_class_hash_v1_path.exists()
            && compiled_class_hash_v2_path.exists()
    };

    with_file_lock(&lock_file_path(contract), is_fresh, || {
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
        compute_and_write_compiled_class_hashes(contract, &casm);

        eprintln!(
            "[compile_cache] Compiled and cached {contract:?} in {:.1}s",
            start.elapsed().as_secs_f64()
        );
    });
}

/// Ensures compiled class hashes are cached for ERC20 (which uses committed CASM artifacts).
/// Uses the CASM file content hash as cache key so that the hashes are recomputed only when
/// the committed artifact changes.
pub fn ensure_erc20_compiled_class_hashes(contract: &FeatureContract, casm_path: &str) {
    assert!(matches!(contract, FeatureContract::ERC20(_)));

    let casm_abs = resolve_crate_relative(casm_path);
    let casm_content = fs::read_to_string(&casm_abs)
        .unwrap_or_else(|e| panic!("Cannot read ERC20 CASM at {casm_abs}: {e}"));

    let mut hasher = Sha256::new();
    hasher.update(casm_content.as_bytes());
    let content_hash = format!("{:x}", hasher.finalize());

    let compiled_class_hash_v1_path = cached_compiled_class_hash_path(contract, &HashVersion::V1);
    let compiled_class_hash_v2_path = cached_compiled_class_hash_path(contract, &HashVersion::V2);
    let key_path = cache_dir().join("cairo1/compiled_class_hashes/erc20.cache_key");

    let is_fresh = || {
        compiled_class_hash_v1_path.exists()
            && compiled_class_hash_v2_path.exists()
            && fs::read_to_string(&key_path).is_ok_and(|k| k.trim() == content_hash)
    };

    let lock = cache_dir().join("erc20_compiled_class_hash.lock");
    with_file_lock(&lock, is_fresh, || {
        compute_and_write_compiled_class_hashes(contract, casm_content.as_bytes());
        atomic_write(&key_path, content_hash.as_bytes());
    });
}
