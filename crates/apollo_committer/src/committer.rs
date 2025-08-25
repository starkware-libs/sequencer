use std::sync::Arc;

use apollo_committer_types::committer_types::{StateCommitmentInput, StateCommitment};
use apollo_committer_types::errors::{CommitterError, CommitterResult};
use apollo_infra::component_definitions::{default_component_start_fn, ComponentStarter};
use async_trait::async_trait;
use starknet_committer::block_committer::commit::commit_block;
use starknet_committer::block_committer::input::{ConfigImpl, Input};
use starknet_patricia_storage::map_storage::MapStorage;
use tokio::sync::Mutex;
use tracing::{info, instrument};

/// The Apollo Committer component responsible for committing state changes to storage.
pub struct Committer {
    storage: Arc<Mutex<MapStorage>>,
}

impl Committer {
    pub fn new(storage: MapStorage) -> Self {
        Self { storage: Arc::new(Mutex::new(storage)) }
    }

    #[instrument(skip(self), err)]
    pub async fn commit(&mut self, input: StateCommitmentInput) -> CommitterResult<StateCommitment> {
        let state_diff = input.state_diff.into();
        let config = ConfigImpl::default();
        let committer_input = Input {
            state_diff,
            contracts_trie_root_hash: input.last_state.contracts_trie_root,
            classes_trie_root_hash: input.last_state.classes_trie_root,
            config,
        };
        let mut storage = self.storage.lock().await;
        let forest = commit_block(committer_input, &mut storage)
            .await
            .map_err(|e| CommitterError::BlockCommitment(e.to_string()))?;
        Ok(StateCommitment {
            contracts_trie_root: forest.get_contract_root_hash(),
            classes_trie_root: forest.get_compiled_class_root_hash(),
        })
    }
}

#[async_trait]
impl ComponentStarter for Committer {
    async fn start(&mut self) {
        info!("Starting Committer component");
        default_component_start_fn::<Self>().await;
    }
}
