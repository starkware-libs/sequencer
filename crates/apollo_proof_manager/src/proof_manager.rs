use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use apollo_infra::component_definitions::{default_component_start_fn, ComponentStarter};
use apollo_proof_manager_config::config::ProofManagerConfig;
use async_trait::async_trait;
use lru::LruCache;
use starknet_api::transaction::fields::{Proof, ProofFacts};
use starknet_api::transaction::TransactionHash;
use starknet_types_core::felt::Felt;

use crate::proof_storage::{FsProofStorage, FsProofStorageError, ProofStorage};

/// In-memory LRU cache for proofs, keyed by the (proof facts hash, tx hash) pair.
#[derive(Clone)]
pub struct ProofCache {
    cache: Arc<Mutex<LruCache<(Felt, TransactionHash), Proof>>>,
}

impl ProofCache {
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self { cache: Arc::new(Mutex::new(LruCache::new(capacity))) }
    }

    pub fn get(&self, key: &(Felt, TransactionHash)) -> Option<Proof> {
        let mut guard = self.cache.lock().expect("Failed to lock proof cache.");
        guard.get(key).cloned()
    }

    pub fn insert(&self, key: (Felt, TransactionHash), proof: Proof) {
        let mut guard = self.cache.lock().expect("Failed to lock proof cache.");
        guard.put(key, proof);
    }

    pub fn contains(&self, key: &(Felt, TransactionHash)) -> bool {
        let guard = self.cache.lock().expect("Failed to lock proof cache.");
        guard.contains(key)
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

    pub async fn set_proof(
        &self,
        proof_facts: ProofFacts,
        tx_hash: TransactionHash,
        proof: Proof,
    ) -> Result<(), FsProofStorageError> {
        if self.contains_proof(proof_facts.clone(), tx_hash).await? {
            return Ok(());
        }
        let key = (proof_facts.hash(), tx_hash);
        self.proof_storage.set_proof(key.0, tx_hash, proof.clone()).await?;
        self.cache.insert(key, proof);
        Ok(())
    }

    pub async fn get_proof(
        &self,
        proof_facts: ProofFacts,
        tx_hash: TransactionHash,
    ) -> Result<Option<Proof>, FsProofStorageError> {
        let key = (proof_facts.hash(), tx_hash);
        // Check cache first.
        if let Some(proof) = self.cache.get(&key) {
            return Ok(Some(proof));
        }
        // Fallback to filesystem.
        let proof = self.proof_storage.get_proof(key.0, tx_hash).await?;
        if let Some(proof) = &proof {
            self.cache.insert(key, proof.clone());
        }
        Ok(proof)
    }

    pub async fn contains_proof(
        &self,
        proof_facts: ProofFacts,
        tx_hash: TransactionHash,
    ) -> Result<bool, FsProofStorageError> {
        let key = (proof_facts.hash(), tx_hash);
        // Check cache first.
        if self.cache.contains(&key) {
            return Ok(true);
        }
        // Fallback to filesystem.
        self.proof_storage.contains_proof(key.0, tx_hash).await
    }
}

#[async_trait]
impl ComponentStarter for ProofManager {
    async fn start(&mut self) {
        default_component_start_fn::<Self>().await;
    }
}
