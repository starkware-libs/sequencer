use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use apollo_signature_manager_types::{KeyStore, KeyStoreError};
use google_cloud_gax::options::RequestOptions;
use google_cloud_gax::response::Response;
use google_cloud_secretmanager_v1::client::SecretManagerService;
use google_cloud_secretmanager_v1::model::{
    AccessSecretVersionRequest,
    AccessSecretVersionResponse,
    SecretPayload,
};
use google_cloud_secretmanager_v1::stub;
use starknet_api::crypto::utils::PrivateKey;
use starknet_core::types::Felt;

use super::GsmKeyStore;

const TEST_SECRET_NAME: &str = "projects/test/secrets/key/versions/latest";
const TEST_HEX_KEY: &str = "0x608bf2cdb1ad4138e72d2f82b8c5db9fa182d1883868ae582ed373429b7a133";

// ---------------------------------------------------------------------------
// Stub helpers
// ---------------------------------------------------------------------------

/// Returns a stub that always yields `hex_key` as the secret payload.
fn make_valid_stub(hex_key: &'static str) -> impl stub::SecretManagerService {
    make_counting_stub(hex_key, Arc::new(AtomicUsize::new(0)))
}

/// A stub that counts how many times `access_secret_version` is called.
fn make_counting_stub(
    hex_key: &'static str,
    counter: Arc<AtomicUsize>,
) -> impl stub::SecretManagerService {
    #[derive(Debug)]
    struct CountingStub {
        hex_key: &'static str,
        counter: Arc<AtomicUsize>,
    }
    impl stub::SecretManagerService for CountingStub {
        async fn access_secret_version(
            &self,
            _req: AccessSecretVersionRequest,
            _options: RequestOptions,
        ) -> google_cloud_secretmanager_v1::Result<Response<AccessSecretVersionResponse>> {
            self.counter.fetch_add(1, Ordering::SeqCst);
            let payload =
                SecretPayload::new().set_data(bytes::Bytes::from(self.hex_key.as_bytes()));
            let response = AccessSecretVersionResponse::new().set_payload(payload);
            Ok(Response::from(response))
        }
    }
    CountingStub { hex_key, counter }
}

/// A stub that returns an empty payload (no `payload` field set).
fn make_empty_payload_stub() -> impl stub::SecretManagerService {
    #[derive(Debug)]
    struct EmptyPayloadStub;
    impl stub::SecretManagerService for EmptyPayloadStub {
        async fn access_secret_version(
            &self,
            _req: AccessSecretVersionRequest,
            _options: RequestOptions,
        ) -> google_cloud_secretmanager_v1::Result<Response<AccessSecretVersionResponse>> {
            // payload field is None by default
            Ok(Response::from(AccessSecretVersionResponse::new()))
        }
    }
    EmptyPayloadStub
}

/// A stub that returns an invalid hex string.
fn make_invalid_hex_stub() -> impl stub::SecretManagerService {
    #[derive(Debug)]
    struct InvalidHexStub;
    impl stub::SecretManagerService for InvalidHexStub {
        async fn access_secret_version(
            &self,
            _req: AccessSecretVersionRequest,
            _options: RequestOptions,
        ) -> google_cloud_secretmanager_v1::Result<Response<AccessSecretVersionResponse>> {
            let payload =
                SecretPayload::new().set_data(bytes::Bytes::from_static(b"not-a-hex-string"));
            let response = AccessSecretVersionResponse::new().set_payload(payload);
            Ok(Response::from(response))
        }
    }
    InvalidHexStub
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// A valid hex key is fetched and decoded into a `PrivateKey`.
#[tokio::test]
async fn valid_key_fetch() {
    let client = SecretManagerService::from_stub(
        make_valid_stub(TEST_HEX_KEY),
    );
    let store = GsmKeyStore::from_client(client, TEST_SECRET_NAME.to_string());

    let key = store.get_key().await.expect("should fetch a valid key");
    assert_eq!(
        key,
        PrivateKey(Felt::from_hex(TEST_HEX_KEY).unwrap())
    );
}

/// When the GSM returns a response with no payload, `get_key` returns a
/// descriptive `KeyStoreError::Custom` error.
#[tokio::test]
async fn empty_payload_error() {
    let client = SecretManagerService::from_stub(
        make_empty_payload_stub(),
    );
    let store = GsmKeyStore::from_client(client, TEST_SECRET_NAME.to_string());

    let err = store.get_key().await.expect_err("expected an error for empty payload");
    assert!(
        matches!(err, KeyStoreError::Custom(ref msg) if msg.contains("empty payload")),
        "unexpected error: {err}"
    );
}

/// When the stored secret is not valid hex, `get_key` returns an error that
/// mentions the bad input.
#[tokio::test]
async fn invalid_hex_error() {
    let client = SecretManagerService::from_stub(
        make_invalid_hex_stub(),
    );
    let store = GsmKeyStore::from_client(client, TEST_SECRET_NAME.to_string());

    let err = store.get_key().await.expect_err("expected an error for invalid hex");
    assert!(
        matches!(err, KeyStoreError::Custom(ref msg) if msg.contains("Invalid key hex")),
        "unexpected error: {err}"
    );
}

/// The key is cached after the first fetch: the stub is called exactly once
/// even when `get_key` is invoked twice.
#[tokio::test]
async fn key_is_cached() {
    let counter = Arc::new(AtomicUsize::new(0));
    let client = SecretManagerService::from_stub(
        make_counting_stub(TEST_HEX_KEY, Arc::clone(&counter)),
    );
    let store = GsmKeyStore::from_client(client, TEST_SECRET_NAME.to_string());

    let key1 = store.get_key().await.expect("first fetch should succeed");
    let key2 = store.get_key().await.expect("second fetch should succeed");
    assert_eq!(key1, key2, "both fetches should return the same key");
    assert_eq!(counter.load(Ordering::SeqCst), 1, "stub should be called exactly once");
}

// ---------------------------------------------------------------------------
// Optional emulator-based integration test
// ---------------------------------------------------------------------------
//
// To run a full end-to-end test against the Secret Manager emulator:
//
//  1. Start the emulator: gcloud beta emulators secretmanager start --host-port=0.0.0.0:8095
//
//  2. Seed a secret: export SECRETMANAGER_EMULATOR_HOST=localhost:8095 gcloud secrets create
//     validator-key --data-file=<(echo -n "0xabc...")
//
//  3. Run the test: cargo test -p apollo_signature_manager --features gsm -- gsm_emulator
//
// The emulator test is gated on a runtime env-var so it never runs in CI
// unless that var is set.
//
// #[tokio::test]
// #[ignore = "requires local Secret Manager emulator"]
// async fn gsm_emulator() {
//     let secret_name = std::env::var("GSM_TEST_SECRET_NAME")
//         .expect("GSM_TEST_SECRET_NAME must be set");
//     let store = GsmKeyStore::new(secret_name).await.expect("GsmKeyStore::new");
//     let _ = store.get_key().await.expect("get_key from emulator");
// }
