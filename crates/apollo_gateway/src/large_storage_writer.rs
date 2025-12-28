use async_trait::async_trait;
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use starknet_api::transaction::fields::Proof;
use starknet_types_core::felt::Felt;
use thiserror::Error;
/// Trait for writing proof facts and proofs to large storage systems.
/// Implementations should be thread-safe (Send + Sync).
#[cfg_attr(any(feature = "testing", test), automock)]
#[async_trait]
pub trait LargeStorageWriterTrait: Send + Sync {
    async fn set_proof(&self, facts_hash: Felt, proof: Proof) -> Result<(), LargeStorageError>;
}

#[derive(Debug, Error)]
pub enum LargeStorageError {
    #[error("Large storage write error: {0}")]
    WriteError(String),
}

#[derive(Clone, Default)]
// TODO(Einat): Add GCS related fields.
pub struct LargeStorageWriter;

impl LargeStorageWriter {
    pub fn new() -> Self {
        // TODO(Einat): connect to GCS client.
        Self
    }
}

#[async_trait]
impl LargeStorageWriterTrait for LargeStorageWriter {
    async fn set_proof(&self, _facts_hash: Felt, _proof: Proof) -> Result<(), LargeStorageError> {
        // TODO(Einat): Write proof to GCS.
        Ok(())
    }
}
