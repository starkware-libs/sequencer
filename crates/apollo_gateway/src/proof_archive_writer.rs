use apollo_gateway_config::config::ProofArchiveWriterConfig;
use async_trait::async_trait;
use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::objects::upload::{Media, UploadObjectRequest, UploadType};
use google_cloud_storage::http::Error as GcsError;
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use starknet_api::transaction::fields::{Proof, ProofFacts};
use thiserror::Error;

// The expected error code for precondition failed errors when using `if_generation_match` in GCS.
const GCS_ERROR_CODE_PRECONDITION_FAILED: u16 = 412;

/// Trait for writing proof facts and proofs to large storage systems.
/// Implementations should be thread-safe (Send + Sync).
#[cfg_attr(any(feature = "testing", test), automock)]
#[async_trait]
pub trait ProofArchiveWriterTrait: Send + Sync {
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

#[derive(Clone)]
pub struct GcsProofArchiveWriter {
    config: ProofArchiveWriterConfig,
    client: Client,
}

impl GcsProofArchiveWriter {
    pub async fn new(config: ProofArchiveWriterConfig) -> Self {
        let client = Client::new(
            ClientConfig::default().with_auth().await.expect("Failed to create GCS client config"),
        );
        Self { config, client }
    }
}

#[async_trait]
impl ProofArchiveWriterTrait for GcsProofArchiveWriter {
    // TODO(Einat): Add retry mechanism.
    async fn set_proof(
        &self,
        proof_facts: ProofFacts,
        proof: Proof,
    ) -> Result<(), ProofArchiveError> {
        let facts_hash = proof_facts.hash();
        let proof_bytes: Vec<u8> = proof.0.iter().flat_map(|&val| val.to_be_bytes()).collect();
        let object_name = format!("proofs/{}", facts_hash);

        let result = self
            .client
            .upload_object(
                &UploadObjectRequest {
                    bucket: self.config.bucket_name.clone(),
                    // Only write if the object does not already exist.
                    if_generation_match: Some(0),
                    ..Default::default()
                },
                proof_bytes,
                &UploadType::Simple(Media::new(object_name)),
            )
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(GcsError::Response(ref err)) if err.code == GCS_ERROR_CODE_PRECONDITION_FAILED => {
                // Precondition failed: object already exists. This is expected if the proof already
                // exists in the archive.
                Ok(())
            }
            Err(e) => Err(ProofArchiveError::WriteError(format!("Failed to upload to GCS: {}", e))),
        }
    }
}

/// No-op proof archive writer that does nothing.
/// Used in tests and when proof archiving is disabled.
#[derive(Clone, Default)]
pub struct NoOpProofArchiveWriter;

#[async_trait]
impl ProofArchiveWriterTrait for NoOpProofArchiveWriter {
    async fn set_proof(
        &self,
        _proof_facts: ProofFacts,
        _proof: Proof,
    ) -> Result<(), ProofArchiveError> {
        // No-op: do nothing in test environments.
        Ok(())
    }
}
