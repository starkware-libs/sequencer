use std::num::NonZeroUsize;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use validator::Validate;

/// Default cache size for the proof manager (number of proofs to keep in memory).
const DEFAULT_CACHE_SIZE: usize = 500;

/// Configuration for the proof manager.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Validate)]
pub struct ProofManagerConfig {
    pub persistent_root: PathBuf,
    pub cache_size: NonZeroUsize,
}

impl Default for ProofManagerConfig {
    fn default() -> Self {
        Self {
            persistent_root: "/data/proofs".into(),
            cache_size: NonZeroUsize::new(DEFAULT_CACHE_SIZE)
                .expect("proof cache size must be non-zero"),
        }
    }
}
