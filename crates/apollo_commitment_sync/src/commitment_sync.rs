// use apollo_commitment_sync_types::errors::CommitmentSyncResult;
use std::collections::HashMap;
use std::sync::Arc;

use apollo_commitment_sync_types::errors::CommitmentSyncResult;
use apollo_commitment_sync_types::types::CommitmentInput;
use apollo_committer_types::committer_types::StateCommitment;
use apollo_committer_types::communication::CommitterClient;
use apollo_infra::component_definitions::{default_component_start_fn, ComponentStarter};
use async_trait::async_trait;
use starknet_api::block::{BlockHash, BlockNumber};
use tracing::info;
/// The Apollo CommitmentSync component responsible for managing asynchronous state commitments.
pub struct CommitmentSync {
    #[allow(dead_code)]
    pub(crate) committer_client: Arc<dyn CommitterClient>,
    #[allow(dead_code)]
    pub(crate) block_hashes: HashMap<BlockNumber, BlockHash>,
    #[allow(dead_code)]
    pub(crate) state_roots: HashMap<BlockNumber, StateCommitment>,
}

impl CommitmentSync {
    pub fn new(committer_client: Arc<dyn CommitterClient>) -> Self {
        Self { committer_client, block_hashes: HashMap::new(), state_roots: HashMap::new() }
    }

    pub fn commit(&mut self, _input: CommitmentInput) -> CommitmentSyncResult<()> {
        todo!("Implement commit logic");
    }

    pub fn get_block_hash(
        &self,
        block_number: BlockNumber,
    ) -> CommitmentSyncResult<Option<BlockHash>> {
        Ok(self.block_hashes.get(&block_number).copied())
    }

    pub fn get_state_commitment(
        &self,
        block_number: BlockNumber,
    ) -> CommitmentSyncResult<Option<StateCommitment>> {
        Ok(self.state_roots.get(&block_number).copied())
    }
}

#[async_trait]
impl ComponentStarter for CommitmentSync {
    async fn start(&mut self) {
        info!("Starting CommitmentSync component");
        default_component_start_fn::<Self>().await;
    }
}
