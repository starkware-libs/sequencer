use std::error::Error;
use std::fs::OpenOptions;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use proving_utils::proof_encoding::{ProofBytes, ProofEncodingError};
use starknet_api::transaction::fields::{Proof, ProofFacts};
use starknet_types_core::felt::Felt;
use thiserror::Error;

#[cfg(test)]
#[path = "proof_storage_test.rs"]
mod proof_storage_test;

pub trait ProofStorage: Send + Sync {
    type Error: Error;
    fn set_proof(&self, proof_facts: ProofFacts, proof: Proof) -> Result<(), Self::Error>;
    fn get_proof(&self, proof_facts: ProofFacts) -> Result<Option<Proof>, Self::Error>;
    fn contains_proof(&self, proof_facts: ProofFacts) -> Result<bool, Self::Error>;
}

#[derive(Debug, Error)]
pub enum FsProofStorageError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error("Proof for facts_hash {facts_hash} not found.")]
    ProofNotFound { facts_hash: Felt },
    #[error(transparent)]
    ProofEncodingError(#[from] ProofEncodingError),
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

    /// Returns the directory that will hold the proof of a certain proof facts hash.
    /// For a proof facts hash: 0xa1b2c3d4... (rest of hash), the structure is:
    /// a1/
    /// └── b2/
    ///     └── a1b2c3d4.../
    fn get_proof_dir(&self, facts_hash: Felt) -> PathBuf {
        let facts_hash = hex::encode(facts_hash.to_bytes_be());
        let (first_msb_byte, second_msb_byte, _rest_of_bytes) =
            (&facts_hash[..2], &facts_hash[2..4], &facts_hash[4..]);
        PathBuf::from(first_msb_byte).join(second_msb_byte).join(facts_hash)
    }

    fn get_persistent_dir(&self, facts_hash: Felt) -> PathBuf {
        self.persistent_root.join(self.get_proof_dir(facts_hash))
    }

    fn get_persistent_dir_with_create(&self, facts_hash: Felt) -> FsProofStorageResult<PathBuf> {
        let path = self.get_persistent_dir(facts_hash);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        Ok(path)
    }

    fn create_tmp_dir(
        &self,
        facts_hash: Felt,
    ) -> FsProofStorageResult<(tempfile::TempDir, PathBuf)> {
        // Compute the final persistent directory for this `facts_hash`
        let persistent_dir = self.get_persistent_dir(facts_hash);
        let parent_dir = persistent_dir
            .parent()
            .expect("Proof persistent dir should have a parent")
            .to_path_buf();
        std::fs::create_dir_all(&parent_dir)?;
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
    fn write_proof_to_file(&self, path: &Path, proof: &Proof) -> FsProofStorageResult<()> {
        let path = path.join("proof");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Open a file for writing, deleting any existing content.
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
            .expect("Failing to open file with given options is impossible");

        let mut writer = BufWriter::new(file);
        let proof_bytes = ProofBytes::try_from(proof.clone())?;

        // Single write.
        writer.write_all(&proof_bytes.0)?;
        writer.flush()?;
        Ok(())
    }

    /// Reads a proof from a file in binary format.
    fn read_proof_from_file(&self, facts_hash: Felt) -> FsProofStorageResult<Proof> {
        let file_path = self.get_persistent_dir(facts_hash).join("proof");
        let mut file = std::fs::File::open(file_path)?;

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        let proof_bytes = ProofBytes(buffer);
        Ok(proof_bytes.into())
    }

    fn write_proof_atomically(&self, facts_hash: Felt, proof: Proof) -> FsProofStorageResult<()> {
        // Write proof to a temporary directory.
        let (_tmp_root, tmp_dir) = self.create_tmp_dir(facts_hash)?;
        self.write_proof_to_file(&tmp_dir, &proof)?;

        // Atomically rename directory to persistent one.
        let persistent_dir = self.get_persistent_dir_with_create(facts_hash)?;
        std::fs::rename(tmp_dir, persistent_dir)?;
        Ok(())
    }

    fn contains_proof_by_hash(&self, facts_hash: Felt) -> bool {
        self.get_persistent_dir(facts_hash).exists()
    }
}

impl ProofStorage for FsProofStorage {
    type Error = FsProofStorageError;

    fn set_proof(&self, proof_facts: ProofFacts, proof: Proof) -> Result<(), Self::Error> {
        let facts_hash = proof_facts.hash();

        if self.contains_proof_by_hash(facts_hash) {
            return Ok(());
        }
        self.write_proof_atomically(facts_hash, proof)
    }

    fn get_proof(&self, proof_facts: ProofFacts) -> Result<Option<Proof>, Self::Error> {
        let facts_hash = proof_facts.hash();
        if !self.contains_proof_by_hash(facts_hash) {
            return Ok(None);
        }

        match self.read_proof_from_file(facts_hash) {
            Ok(proof) => Ok(Some(proof)),
            Err(FsProofStorageError::IoError(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    fn contains_proof(&self, proof_facts: ProofFacts) -> Result<bool, Self::Error> {
        Ok(self.contains_proof_by_hash(proof_facts.hash()))
    }
}
