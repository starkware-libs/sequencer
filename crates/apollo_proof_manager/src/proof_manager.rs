use apollo_proof_manager_config::config::ProofManagerConfig;
use starknet_api::transaction::fields::{Proof, ProofFacts};

use crate::proof_storage::{FsProofStorage, FsProofStorageError, ProofStorage};
/// Proof manager that wraps filesystem-based proof storage.
pub struct ProofManager {
    pub proof_storage: FsProofStorage,
    // TODO(Einat): Add cache.
}

impl ProofManager {
    pub fn new(config: ProofManagerConfig) -> Self {
        let proof_storage =
            FsProofStorage::new(config.persistent_root).expect("Failed to create proof storage.");
        Self { proof_storage }
    }
}

impl ProofStorage for ProofManager {
    type Error = FsProofStorageError;

    fn set_proof(&self, proof_facts: ProofFacts, proof: Proof) -> Result<(), Self::Error> {
        self.proof_storage.set_proof(proof_facts, proof)
    }

    fn get_proof(&self, proof_facts: ProofFacts) -> Result<Option<Proof>, Self::Error> {
        self.proof_storage.get_proof(proof_facts)
    }

    fn contains_proof(&self, proof_facts: ProofFacts) -> Result<bool, Self::Error> {
        self.proof_storage.contains_proof(proof_facts)
    }
}
