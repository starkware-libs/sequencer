use std::time::Duration;

use apollo_gateway_config::config::ProofArchiveWriterConfig;
use async_trait::async_trait;
use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::objects::upload::{Media, UploadObjectRequest, UploadType};
use google_cloud_storage::http::Error as GcsError;
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use starknet_api::transaction::fields::{Proof, ProofFacts};
use thiserror::Error;
use tokio::sync::OnceCell;
use tracing::warn;

// The expected error code for precondition failed errors when using `if_generation_match` in GCS.
const GCS_ERROR_CODE_PRECONDITION_FAILED: u16 = 412;

// Retry parameters: exponential backoff starting at 100ms, doubling each attempt, up to 3 retries.
const MAX_RETRIES: u32 = 3;
const RETRY_BASE_DELAY: Duration = Duration::from_millis(100);

/// Trait for writing proof facts and proofs to large storage systems.
/// Implementations should be thread-safe (Send + Sync).
#[cfg_attr(any(feature = "testing", test), automock)]
#[async_trait]
pub trait ProofArchiveWriterTrait: Send + Sync {
    /// Connects to the storage backend. Should be called once during component startup.
    async fn connect(&self);

    async fn set_proof(
        &self,
        proof_facts: ProofFacts,
        proof: Proof,
    ) -> Result<(), ProofArchiveError>;
}

#[derive(Debug, Error)]
pub enum ProofArchiveError {
    #[error("Proof archive write error: {0}")]
    WriteError(String),
}

/// Returns `true` if a GCS error is transient and the operation should be retried.
fn is_retriable(err: &GcsError) -> bool {
    match err {
        // Use the GCS library's built-in retriability check (covers HTTP 408, 429, 500-599).
        GcsError::Response(response_err) => response_err.is_retriable(),
        // Network-level errors (connection reset, timeout, DNS failures) are transient.
        GcsError::HttpClient(_) | GcsError::HttpMiddleware(_) => true,
        _ => false,
    }
}

pub struct GcsProofArchiveWriter {
    config: ProofArchiveWriterConfig,
    client: OnceCell<Client>,
}

impl GcsProofArchiveWriter {
    pub fn new(config: ProofArchiveWriterConfig) -> Self {
        Self { config, client: OnceCell::new() }
    }
}

#[async_trait]
impl ProofArchiveWriterTrait for GcsProofArchiveWriter {
    async fn connect(&self) {
        self.client
            .get_or_init(|| async {
                let client_config = ClientConfig::default()
                    .with_auth()
                    .await
                    .expect("Failed to create GCS client config");
                Client::new(client_config)
            })
            .await;
    }

    async fn set_proof(
        &self,
        proof_facts: ProofFacts,
        proof: Proof,
    ) -> Result<(), ProofArchiveError> {
        let client = self.client.get().expect("GCS client not connected. Call connect() first.");
        let facts_hash = proof_facts.hash();
        let proof_bytes: Vec<u8> = proof.0.iter().flat_map(|&val| val.to_be_bytes()).collect();
        let object_name = format!("proofs/{}", facts_hash);

        let mut last_err = None;
        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                tokio::time::sleep(RETRY_BASE_DELAY * 2u32.pow(attempt - 1)).await;
            }

            let result = client
                .upload_object(
                    &UploadObjectRequest {
                        bucket: self.config.bucket_name.clone(),
                        // Only write if the object does not already exist.
                        if_generation_match: Some(0),
                        ..Default::default()
                    },
                    proof_bytes.clone(),
                    &UploadType::Simple(Media::new(object_name.clone())),
                )
                .await;

            match result {
                Ok(_) => return Ok(()),
                Err(GcsError::Response(ref err))
                    if err.code == GCS_ERROR_CODE_PRECONDITION_FAILED =>
                {
                    // Object already exists — this is expected and not an error.
                    return Ok(());
                }
                Err(ref err) if is_retriable(err) => {
                    warn!(
                        "GCS upload attempt {}/{} failed with retriable error: {err}.",
                        attempt + 1,
                        MAX_RETRIES + 1,
                    );
                    last_err = Some(result.unwrap_err());
                }
                Err(e) => {
                    return Err(ProofArchiveError::WriteError(format!(
                        "Failed to upload to GCS: {e}"
                    )));
                }
            }
        }

        Err(ProofArchiveError::WriteError(format!(
            "Failed to upload to GCS after {} attempts: {}",
            MAX_RETRIES + 1,
            last_err.unwrap(),
        )))
    }
}

/// No-op proof archive writer that does nothing.
/// Used in tests and when proof archiving is disabled.
#[derive(Clone, Default)]
pub struct NoOpProofArchiveWriter;

#[async_trait]
impl ProofArchiveWriterTrait for NoOpProofArchiveWriter {
    async fn connect(&self) {
        // No-op: do nothing in test environments.
    }

    async fn set_proof(
        &self,
        _proof_facts: ProofFacts,
        _proof: Proof,
    ) -> Result<(), ProofArchiveError> {
        // No-op: do nothing in test environments.
        Ok(())
    }
}
