use std::fmt::{self, Debug, Formatter};
use std::str;

use apollo_signature_manager_types::{KeyStore, KeyStoreError, KeyStoreResult};
use async_trait::async_trait;
use google_cloud_gax::client_builder::Error as ClientBuilderError;
use google_cloud_secretmanager_v1::client::SecretManagerService;
use starknet_api::crypto::utils::PrivateKey;
use starknet_core::types::Felt;
use tokio::sync::OnceCell;

/// A key store backed by Google Secret Manager.
///
/// The key is fetched once on first use and cached for the lifetime of this store.
///
/// Authentication uses Application Default Credentials (ADC). On GKE with
/// Workload Identity, the pod's bound service account is used automatically.
pub struct GsmKeyStore {
    client: SecretManagerService,
    /// Full GSM resource name: "projects/{p}/secrets/{s}/versions/{v}"
    secret_name: String,
    cached_key: OnceCell<PrivateKey>,
}

impl GsmKeyStore {
    /// Creates a new `GsmKeyStore` using Application Default Credentials.
    pub async fn new(secret_name: String) -> Result<Self, ClientBuilderError> {
        let client = SecretManagerService::builder().build().await?;
        Ok(Self { client, secret_name, cached_key: OnceCell::new() })
    }

    /// Creates a `GsmKeyStore` from an existing client.
    pub fn from_client(client: SecretManagerService, secret_name: String) -> Self {
        Self { client, secret_name, cached_key: OnceCell::new() }
    }

    async fn fetch_from_gsm(&self) -> KeyStoreResult<PrivateKey> {
        let response = self
            .client
            .access_secret_version()
            .set_name(&self.secret_name)
            .send()
            .await
            .map_err(|e| KeyStoreError::Custom(e.to_string()))?;

        let payload = response
            .payload
            .ok_or_else(|| KeyStoreError::Custom("GSM returned empty payload".into()))?;

        // payload.data is bytes::Bytes - the raw UTF-8 bytes of the stored hex string
        let hex_str = str::from_utf8(&payload.data)
            .map_err(|e| KeyStoreError::Custom(format!("Non-UTF8 secret: {e}")))?
            .trim();

        let felt = Felt::from_hex(hex_str)
            .map_err(|e| KeyStoreError::Custom(format!("Invalid key hex: {e}")))?;

        Ok(PrivateKey(felt))
    }
}

impl Clone for GsmKeyStore {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            secret_name: self.secret_name.clone(),
            // Reset the cache on clone so each clone fetches independently.
            cached_key: OnceCell::new(),
        }
    }
}

impl Debug for GsmKeyStore {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("GsmKeyStore").field("secret_name", &self.secret_name).finish()
    }
}

#[async_trait]
impl KeyStore for GsmKeyStore {
    async fn get_key(&self) -> KeyStoreResult<PrivateKey> {
        self.cached_key.get_or_try_init(|| self.fetch_from_gsm()).await.copied()
    }
}

#[cfg(test)]
#[path = "gsm_key_store_test.rs"]
mod gsm_key_store_test;
