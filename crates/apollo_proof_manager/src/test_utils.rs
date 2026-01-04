use std::path::PathBuf;

use apollo_proof_manager_config::config::ProofManagerConfig;
use tempfile::TempDir;

pub struct FsProofStorageBuilderForTesting {
    config: ProofManagerConfig,
    handle: Option<TempDir>,
}

impl Default for FsProofStorageBuilderForTesting {
    fn default() -> Self {
        let persistent_root_handle = tempfile::tempdir().unwrap();
        let persistent_root = persistent_root_handle.path().to_path_buf();
        let config = ProofManagerConfig { persistent_root };
        Self { config, handle: Some(persistent_root_handle) }
    }
}

impl FsProofStorageBuilderForTesting {
    pub fn with_existing_path(mut self, persistent_path: PathBuf) -> Self {
        self.config.persistent_root = persistent_path;
        self.handle = None;
        self
    }

    pub fn build(self) -> (ProofManagerConfig, Option<TempDir>) {
        let Self { config, handle } = self;
        (config, handle)
    }
}
