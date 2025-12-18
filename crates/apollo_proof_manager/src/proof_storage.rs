use std::convert::TryInto;
use std::error::Error;
use std::fs::OpenOptions;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use starknet_api::transaction::fields::Proof;
use starknet_types_core::felt::Felt;
use thiserror::Error;

pub trait ProofStorage: Send + Sync {
    type Error: Error;
    fn set_proof(&self, facts_hash: Felt, proof: Proof) -> Result<(), Self::Error>;
    fn get_proof(&self, facts_hash: Felt) -> Result<Option<Proof>, Self::Error>;
    fn contains_proof(&self, facts_hash: Felt) -> Result<bool, Self::Error>;
}

#[derive(Debug, Error)]
pub enum FsProofStorageError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error("Proof for facts_hash {facts_hash} not found.")]
    ProofNotFound { facts_hash: Felt },
}

type FsProofStorageResult<T> = Result<T, FsProofStorageError>;

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
    #[allow(dead_code)]
    fn get_proof_dir(&self, facts_hash: Felt) -> PathBuf {
        let facts_hash = hex::encode(facts_hash.to_bytes_be());
        let (first_msb_byte, second_msb_byte, _rest_of_bytes) =
            (&facts_hash[..2], &facts_hash[2..4], &facts_hash[4..]);
        PathBuf::from(first_msb_byte).join(second_msb_byte).join(facts_hash)
    }

    #[allow(dead_code)]
    fn get_persistent_dir(&self, facts_hash: Felt) -> PathBuf {
        self.persistent_root.join(self.get_proof_dir(facts_hash))
    }

    #[allow(dead_code)]
    fn get_persistent_dir_with_create(&self, facts_hash: Felt) -> FsProofStorageResult<PathBuf> {
        let path = self.get_persistent_dir(facts_hash);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        Ok(path)
    }

    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
        // Pre-allocate exactly enough space, 4 bytes per u32.
        let mut buf: Vec<u8> = Vec::with_capacity(proof.len() * std::mem::size_of::<u32>());

        for &value in proof.iter() {
            buf.extend_from_slice(&value.to_be_bytes());
        }

        // Single write.
        writer.write_all(&buf)?;
        writer.flush()?;
        Ok(())
    }

    /// Reads a proof from a file in binary format.
    #[allow(dead_code)]
    fn read_proof_from_file(&self, facts_hash: Felt) -> FsProofStorageResult<Proof> {
        let file_path = self.get_persistent_dir(facts_hash).join("proof");
        let mut file = std::fs::File::open(file_path)?;

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        if buffer.len() % 4 != 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Corrupt file").into());
        }

        let proof_data = buffer
            .chunks_exact(4)
            .map(|c| u32::from_be_bytes(c.try_into().expect("4 bytes should fit in a u32")))
            .collect();

        Ok(Proof(Arc::new(proof_data)))
    }
}
