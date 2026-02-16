use crate::errors::ProvingError;
use crate::proving::bootloader::{
    bootloader_download_url,
    calculate_sha256_hex,
    verify_bootloader_sha256,
    BOOTLOADER_JSON_SHA256,
};

#[test]
fn test_verify_bootloader_sha256_wrong_hash_returns_mismatch_error() {
    let result = verify_bootloader_sha256(b"not the bootloader");
    let err = result.unwrap_err();
    match err {
        ProvingError::BootloaderFileSha256Mismatch { expected, actual } => {
            assert_eq!(expected, BOOTLOADER_JSON_SHA256);
            assert_eq!(actual, calculate_sha256_hex(b"not the bootloader"));
        }
        other => panic!("Expected BootloaderFileSha256Mismatch, got: {other:?}"),
    }
}

/// Downloads the bootloader JSON from the pinned revision and verifies its SHA-256.
#[tokio::test]
async fn test_downloaded_bootloader_sha256() {
    let url = bootloader_download_url();
    let response = reqwest::get(&url).await.expect("Failed to download bootloader");
    let bytes =
        response.error_for_status().expect("HTTP error").bytes().await.expect("Failed to read body");
    let actual_hash = calculate_sha256_hex(&bytes);
    assert_eq!(
        actual_hash, BOOTLOADER_JSON_SHA256,
        "Downloaded bootloader SHA-256 does not match expected hash."
    );
}
