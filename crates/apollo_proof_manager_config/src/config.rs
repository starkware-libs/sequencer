use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::path::PathBuf;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
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

impl SerializeConfig for ProofManagerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param(
                "persistent_root",
                &self.persistent_root,
                "Persistent root for proof storage.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "cache_size",
                &self.cache_size,
                "Number of proofs to cache in memory.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}
