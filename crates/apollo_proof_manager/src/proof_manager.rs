use std::collections::BTreeMap;
use std::path::PathBuf;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::transaction::fields::Proof;
use starknet_types_core::felt::Felt;
use validator::Validate;

use crate::proof_storage::{FsProofStorage, FsProofStorageError, ProofStorage};

/// Configuration for the proof manager.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Validate)]
pub struct ProofManagerConfig {
    pub persistent_root: PathBuf,
}
impl Default for ProofManagerConfig {
    fn default() -> Self {
        Self { persistent_root: "/data/proofs".into() }
    }
}
impl SerializeConfig for ProofManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([ser_param(
            "persistent_root",
            &self.persistent_root,
            "Persistent root for proof storage.",
            ParamPrivacyInput::Public,
        )])
    }
}
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
