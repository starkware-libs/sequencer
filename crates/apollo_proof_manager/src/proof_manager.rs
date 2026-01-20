use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use apollo_infra::component_definitions::{default_component_start_fn, ComponentStarter};
use apollo_proof_manager_config::config::ProofManagerConfig;
use async_trait::async_trait;
use lru::LruCache;
use starknet_api::transaction::fields::{Proof, ProofFacts};
use starknet_types_core::felt::Felt;

use crate::proof_storage::{FsProofStorage, FsProofStorageError, ProofStorage};

/// In-memory LRU cache for proofs, keyed by the hash of ProofFacts.
#[derive(Clone)]
pub struct ProofCache {
    cache: Arc<Mutex<LruCache<Felt, Proof>>>,
}

impl ProofCache {
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self { cache: Arc::new(Mutex::new(LruCache::new(capacity))) }
    }

    pub fn get(&self, facts_hash: &Felt) -> Option<Proof> {
        self.cache.lock().expect("Failed to lock proof cache.").get(facts_hash).cloned()
    }

    pub fn insert(&self, facts_hash: Felt, proof: Proof) {
        self.cache.lock().expect("Failed to lock proof cache.").put(facts_hash, proof);
    }

    pub fn contains(&self, facts_hash: &Felt) -> bool {
        self.cache.lock().expect("Failed to lock proof cache.").contains(facts_hash)
    }
}

/// Proof manager that wraps filesystem-based proof storage with an in-memory cache.
#[derive(Clone)]
pub struct ProofManager {
    pub proof_storage: FsProofStorage,
    cache: ProofCache,
}

impl ProofManager {
    pub fn new(config: ProofManagerConfig) -> Self {
        let proof_storage =
            FsProofStorage::new(config.persistent_root).expect("Failed to create proof storage.");
        Self { proof_storage, cache: ProofCache::new(config.cache_size) }
    }

    pub fn set_proof(
        &self,
        proof_facts: ProofFacts,
        proof: Proof,
    ) -> Result<(), FsProofStorageError> {
        let facts_hash = proof_facts.hash();
        self.cache.insert(facts_hash, proof.clone());
        self.proof_storage.set_proof(facts_hash, proof)
    }

    pub fn get_proof(&self, proof_facts: ProofFacts) -> Result<Option<Proof>, FsProofStorageError> {
        let facts_hash = proof_facts.hash();
        // Check cache first.
        if let Some(proof) = self.cache.get(&facts_hash) {
            return Ok(Some(proof));
        }
        // Fallback to filesystem.
        self.proof_storage.get_proof(facts_hash)
    }

    pub fn contains_proof(&self, proof_facts: ProofFacts) -> Result<bool, FsProofStorageError> {
        let facts_hash = proof_facts.hash();
        // Check cache first.
        if self.cache.contains(&facts_hash) {
            return Ok(true);
        }
        // Fallback to filesystem.
        self.proof_storage.contains_proof(facts_hash)
    }
}

#[async_trait]
impl ComponentStarter for ProofManager {
    async fn start(&mut self) {
        default_component_start_fn::<Self>().await;
    }
}
