use std::path::PathBuf;

use starknet_api::transaction::fields::Proof;
use starknet_types_core::felt::Felt;

use crate::proof_storage::{FsProofStorage, FsProofStorageError, ProofStorage};

/// Configuration for the proof manager.
#[derive(Clone, Debug)]
pub struct ProofManagerConfig {
    pub persistent_root: PathBuf,
}

/// Proof manager that wraps filesystem-based proof storage.
pub struct ProofManager {
    pub proof_storage: FsProofStorage,
    pub proof_manager_config: ProofManagerConfig,
}

impl ProofManager {
    pub fn new(config: ProofManagerConfig, storage: FsProofStorage) -> Self {
        Self { proof_storage: storage, proof_manager_config: config }
    }
}

impl ProofStorage for ProofManager {
    type Error = FsProofStorageError;

    fn set_proof(&self, facts_hash: Felt, proof: Proof) -> Result<(), Self::Error> {
        self.proof_storage.set_proof(facts_hash, proof)
    }

    fn get_proof(&self, facts_hash: Felt) -> Result<Option<Proof>, Self::Error> {
        self.proof_storage.get_proof(facts_hash)
    }

    fn contains_proof(&self, facts_hash: Felt) -> Result<bool, Self::Error> {
        self.proof_storage.contains_proof(facts_hash)
    }
}

pub fn create_proof_manager(config: ProofManagerConfig) -> ProofManager {
    let ProofManagerConfig { persistent_root } = config.clone();
    let fs_proof_storage =
        FsProofStorage::new(persistent_root).expect("Failed to create proof storage.");
    ProofManager::new(config, fs_proof_storage)
}
