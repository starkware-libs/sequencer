use std::error::Error;
use std::path::{Path, PathBuf};

use starknet_api::transaction::fields::Proof;
use starknet_api::transaction::TransactionHash;
use starknet_types_core::felt::Felt;
use thiserror::Error;
use tokio::io::AsyncWriteExt;

#[cfg(test)]
#[path = "proof_storage_test.rs"]
mod proof_storage_test;

#[async_trait::async_trait]
pub trait ProofStorage: Send + Sync {
    type Error: Error;
    async fn set_proof(
        &self,
        facts_hash: Felt,
        tx_hash: TransactionHash,
        proof: Proof,
    ) -> Result<(), Self::Error>;
    async fn get_proof(
        &self,
        facts_hash: Felt,
        tx_hash: TransactionHash,
    ) -> Result<Option<Proof>, Self::Error>;
    async fn contains_proof(
        &self,
        facts_hash: Felt,
        tx_hash: TransactionHash,
    ) -> Result<bool, Self::Error>;
}

#[derive(Debug, Error)]
pub enum FsProofStorageError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error("Proof for facts_hash {facts_hash} not found.")]
    ProofNotFound { facts_hash: Felt },
}

type FsProofStorageResult<T> = Result<T, FsProofStorageError>;

#[derive(Clone)]
pub struct FsProofStorage {
    persistent_root: PathBuf,
}

impl FsProofStorage {
    // TODO(Einat): consider code sharing with class storage.
    pub fn new(persistent_root: PathBuf) -> Result<Self, std::io::Error> {
        std::fs::create_dir_all(&persistent_root)?;
        Ok(Self { persistent_root })
    }

    /// Returns the directory that will hold the proof of a certain proof facts hash and tx hash.
    /// For a proof facts hash: 0xa1b2c3d4... and tx hash, the structure is:
    /// a1/
    /// └── b2/
    ///     └── a1b2c3d4.../
    ///         └── <tx_hash_hex>/
    fn get_proof_dir(&self, facts_hash: Felt, tx_hash: TransactionHash) -> PathBuf {
        let facts_hash_hex = hex::encode(facts_hash.to_bytes_be());
        let (first_msb_byte, second_msb_byte, _rest) =
            (&facts_hash_hex[..2], &facts_hash_hex[2..4], &facts_hash_hex[4..]);
        let tx_hash_hex = hex::encode(tx_hash.0.to_bytes_be());
        PathBuf::from(first_msb_byte).join(second_msb_byte).join(&facts_hash_hex).join(tx_hash_hex)
    }

    fn get_persistent_dir(&self, facts_hash: Felt, tx_hash: TransactionHash) -> PathBuf {
        self.persistent_root.join(self.get_proof_dir(facts_hash, tx_hash))
    }

    async fn get_persistent_dir_with_create(
        &self,
        facts_hash: Felt,
        tx_hash: TransactionHash,
    ) -> FsProofStorageResult<PathBuf> {
        let path = self.get_persistent_dir(facts_hash, tx_hash);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        Ok(path)
    }

    async fn create_tmp_dir(
        &self,
        facts_hash: Felt,
        tx_hash: TransactionHash,
    ) -> FsProofStorageResult<(tempfile::TempDir, PathBuf)> {
        // Compute the final persistent directory for this `facts_hash` and `tx_hash`
        let persistent_dir = self.get_persistent_dir(facts_hash, tx_hash);
        let parent_dir = persistent_dir
            .parent()
            .expect("Proof persistent dir should have a parent")
            .to_path_buf();
        tokio::fs::create_dir_all(&parent_dir).await?;
        // Create a temporary directory under the parent of the final persistent directory to ensure
        // `rename` will be atomic.
        let tmp_root = tempfile::tempdir_in(&parent_dir)?;
        // Get the leaf directory name of the final persistent directory.
        let leaf = persistent_dir.file_name().expect("Proof dir leaf should exist");
        // Create the temporary directory under the temporary root.
        let tmp_dir = tmp_root.path().join(leaf);
        // Returning `TempDir` since without it the handle would drop immediately and the temp
        // directory would be removed before writes/rename.
        Ok((tmp_root, tmp_dir))
    }

    /// Writes a proof to a file in binary format.
    /// The file is named `proof` inside the given directory.
    async fn write_proof_to_file(&self, path: &Path, proof: &Proof) -> FsProofStorageResult<()> {
        let path = path.join("proof");
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let mut file = tokio::fs::File::create(&path).await?;
        file.write_all(&proof.0).await?;
        file.flush().await?;
        Ok(())
    }

    /// Reads a proof from a file in binary format.
    async fn read_proof_from_file(
        &self,
        facts_hash: Felt,
        tx_hash: TransactionHash,
    ) -> FsProofStorageResult<Proof> {
        let file_path = self.get_persistent_dir(facts_hash, tx_hash).join("proof");
        let buffer = tokio::fs::read(&file_path).await?;
        Ok(Proof::from(buffer))
    }

    async fn write_proof_atomically(
        &self,
        facts_hash: Felt,
        tx_hash: TransactionHash,
        proof: Proof,
    ) -> FsProofStorageResult<()> {
        // Write proof to a temporary directory.
        let (_tmp_root, tmp_dir) = self.create_tmp_dir(facts_hash, tx_hash).await?;
        self.write_proof_to_file(&tmp_dir, &proof).await?;

        // Atomically rename directory to persistent one.
        // If a concurrent write already placed the proof at the persistent path, the rename
        // will fail (e.g. ENOTEMPTY on Linux). Since proofs are deterministic for a given
        // facts_hash and tx_hash, the existing proof is identical and we can safely treat this as
        // success.
        let persistent_dir = self.get_persistent_dir_with_create(facts_hash, tx_hash).await?;
        match tokio::fs::rename(&tmp_dir, &persistent_dir).await {
            Ok(()) => Ok(()),
            Err(_)
                if tokio::fs::try_exists(persistent_dir.join("proof")).await.unwrap_or(false) =>
            {
                Ok(())
            }
            Err(e) => Err(e.into()),
        }
    }
}

#[async_trait::async_trait]
impl ProofStorage for FsProofStorage {
    type Error = FsProofStorageError;

    async fn set_proof(
        &self,
        facts_hash: Felt,
        tx_hash: TransactionHash,
        proof: Proof,
    ) -> Result<(), Self::Error> {
        self.write_proof_atomically(facts_hash, tx_hash, proof).await
    }

    async fn get_proof(
        &self,
        facts_hash: Felt,
        tx_hash: TransactionHash,
    ) -> Result<Option<Proof>, Self::Error> {
        if !self.contains_proof(facts_hash, tx_hash).await? {
            return Ok(None);
        }

        match self.read_proof_from_file(facts_hash, tx_hash).await {
            Ok(proof) => Ok(Some(proof)),
            Err(FsProofStorageError::IoError(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    async fn contains_proof(
        &self,
        facts_hash: Felt,
        tx_hash: TransactionHash,
    ) -> Result<bool, Self::Error> {
        Ok(tokio::fs::try_exists(self.get_persistent_dir(facts_hash, tx_hash)).await?)
    }
}
