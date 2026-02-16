//! Bootloader JSON download, caching, and SHA-256 verification.
//!
//! The bootloader is downloaded from a pinned proving-utils revision and cached locally
//! under `target/starknet_os_runner/<rev>/`. The downloaded file is verified against a
//! SHA-256 hash before being persisted; subsequent cache hits skip the hash check since
//! the cache path is keyed by revision.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use apollo_infra_utils::path::project_path;
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use tokio::sync::OnceCell;
use tokio_retry::strategy::ExponentialBackoff;
use tokio_retry::RetryIf;
use tracing::info;

use crate::errors::ProvingError;

/// Bootloader program file name.
pub(crate) const BOOTLOADER_FILE: &str = "simple_bootloader_compiled.json";
/// SHA-256 of the full bootloader JSON file from proving-utils.
pub(crate) const BOOTLOADER_JSON_SHA256: &str =
    "f6d235eb6a7f97038105ed9b6e0e083b11def61c664a17fe157135f9615efc76";
/// Pinned proving-utils revision that contains the bootloader JSON.
const PROVING_UTILS_REV: &str = "e16f9d0";
/// Path to the bootloader JSON within the proving-utils repository.
const BOOTLOADER_REPO_PATH: &str = "crates/cairo-program-runner-lib/resources/compiled_programs/\
                                    bootloaders/simple_bootloader_compiled.json";

/// Stores the resolved bootloader path after the first successful download/cache hit.
static BOOTLOADER_PATH: OnceCell<PathBuf> = OnceCell::const_new();

/// Resolves a local path for the bootloader JSON, downloading and verifying it if needed.
///
/// Uses a [`OnceCell`] so that concurrent callers share a single download attempt. The first
/// call performs the download; subsequent calls return the cached path immediately.
///
/// The cache is keyed by the proving-utils revision: the file is stored under
/// `target/starknet_os_runner/<rev>/`. On a cache hit the file is returned immediately
/// (it was already verified when first downloaded). On a cache miss the file is downloaded
/// and verified.
pub async fn resolve_bootloader_path() -> Result<PathBuf, ProvingError> {
    BOOTLOADER_PATH.get_or_try_init(download_and_cache_bootloader).await.cloned()
}

async fn download_and_cache_bootloader() -> Result<PathBuf, ProvingError> {
    let bootloader_path = bootloader_cache_path()?;

    if bootloader_path.exists() {
        return Ok(bootloader_path);
    }

    let download_url = bootloader_download_url();
    info!(
        bootloader_url = %download_url,
        proving_utils_rev = PROVING_UTILS_REV,
        cache_path = %bootloader_path.display(),
        "Downloading bootloader JSON."
    );
    download_with_retry(&download_url, &bootloader_path).await?;
    Ok(bootloader_path)
}

/// Downloads the bootloader with exponential backoff retry on transient download errors.
/// SHA-256 mismatches are not retried (they indicate a permanent problem).
async fn download_with_retry(url: &str, path: &Path) -> Result<(), ProvingError> {
    let strategy = ExponentialBackoff::from_millis(500).max_delay(Duration::from_secs(5)).take(3);
    RetryIf::spawn(
        strategy,
        || download_bootloader_to_path(url, path),
        |e: &ProvingError| matches!(e, ProvingError::DownloadBootloader(_)),
    )
    .await
}

fn bootloader_cache_path() -> Result<PathBuf, ProvingError> {
    let project_root = project_path().map_err(ProvingError::ResolveProjectRootPath)?;
    Ok(project_root
        .join("target")
        .join("starknet_os_runner")
        .join(PROVING_UTILS_REV)
        .join(BOOTLOADER_FILE))
}

pub(crate) fn bootloader_download_url() -> String {
    format!(
        "https://raw.githubusercontent.com/starkware-libs/proving-utils/{PROVING_UTILS_REV}/\
         {BOOTLOADER_REPO_PATH}"
    )
}

async fn download_bootloader_to_path(
    url: &str,
    bootloader_path: &Path,
) -> Result<(), ProvingError> {
    let response = reqwest::get(url).await.map_err(ProvingError::DownloadBootloader)?;
    let bootloader_bytes = response
        .error_for_status()
        .map_err(ProvingError::DownloadBootloader)?
        .bytes()
        .await
        .map_err(ProvingError::DownloadBootloader)?;

    verify_bootloader_sha256(&bootloader_bytes)?;
    write_bootloader_cache(bootloader_path, &bootloader_bytes)
}

fn write_bootloader_cache(
    bootloader_path: &Path,
    bootloader_bytes: &[u8],
) -> Result<(), ProvingError> {
    let parent_dir = bootloader_path.parent().ok_or_else(|| {
        ProvingError::InvalidBootloaderPath { path: bootloader_path.display().to_string() }
    })?;
    std::fs::create_dir_all(parent_dir).map_err(|source| {
        ProvingError::CreateBootloaderCacheDir { path: parent_dir.display().to_string(), source }
    })?;

    atomic_write(parent_dir, bootloader_path, bootloader_bytes)
}

/// Writes bytes to the target path atomically via a temporary file and `persist`.
fn atomic_write(dir: &Path, target: &Path, bytes: &[u8]) -> Result<(), ProvingError> {
    let mut temp_file = NamedTempFile::new_in(dir).map_err(ProvingError::CreateTempFile)?;
    temp_file.write_all(bytes).map_err(|source| ProvingError::WriteBootloaderCache {
        path: target.display().to_string(),
        source,
    })?;
    temp_file.persist(target).map_err(|source| ProvingError::PersistBootloaderCache {
        path: target.display().to_string(),
        source: source.error,
    })?;
    Ok(())
}

pub(crate) fn verify_bootloader_sha256(bootloader_bytes: &[u8]) -> Result<(), ProvingError> {
    let actual = calculate_sha256_hex(bootloader_bytes);
    if actual != BOOTLOADER_JSON_SHA256 {
        return Err(ProvingError::BootloaderFileSha256Mismatch {
            expected: BOOTLOADER_JSON_SHA256.to_string(),
            actual,
        });
    }
    Ok(())
}

pub(crate) fn calculate_sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
