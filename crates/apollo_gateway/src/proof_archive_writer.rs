use async_trait::async_trait;
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use starknet_api::transaction::fields::{Proof, ProofFacts};
use thiserror::Error;
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

#[derive(Clone, Default)]
// TODO(Einat): Add GCS related fields.
pub struct GcsProofArchiveWriter;

impl GcsProofArchiveWriter {
    pub fn new() -> Self {
        // TODO(Einat): connect to GCS client.
        Self
    }
}

#[async_trait]
impl ProofArchiveWriterTrait for GcsProofArchiveWriter {
    async fn set_proof(
        &self,
        _proof_facts: ProofFacts,
        _proof: Proof,
    ) -> Result<(), ProofArchiveError> {
        // TODO(Einat): Write proof to GCS.
        Ok(())
    }
}
