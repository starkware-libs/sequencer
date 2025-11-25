use std::sync::Arc;

use apollo_committer_types::communication::SharedCommitterClient;
use apollo_l1_provider_types::SharedL1ProviderClient;
use apollo_state_sync_types::state_sync_types::SyncBlock;
use futures::channel::mpsc::Receiver;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::block_hash::block_hash_calculator::PartialBlockHashComponents;
use starknet_api::hash::StateRoots;
use starknet_api::state::ThinStateDiff;

pub(crate) struct BlockHashManager {
    block_hash_offset: BlockNumber,
    l1_provider_client: SharedL1ProviderClient,
    batcher_storage_reader: Arc<dyn BatcherStorageReaderTraitForBMH>,
    storage_writer: Box<dyn BlockHashManagerStorageWriterTrait>,
    storage_reader: Arc<dyn BlockHashManagerStorageReaderTrait>,
    committer_client: SharedCommitterClient,
    tasks_channel: Receiver<BlockHashManagerInput>,
}

pub(crate) enum BlockHashManagerInput {
    SyncBlock(SyncBlock),
    CreatedBlock(CreatedBlock),
}

pub(crate) struct CreatedBlock {
    pub(crate) block_number: BlockNumber,
    pub(crate) state_diff: ThinStateDiff,
    pub(crate) partial_block_hash_components: PartialBlockHashComponents,
}

pub(crate) trait BlockHashManagerStorageReaderTrait {
    fn get_block_hash(
        &self,
        height: &BlockNumber,
    ) -> apollo_storage::StorageResult<Option<BlockHash>>;

    fn get_state_roots(
        &self,
        height: &BlockNumber,
    ) -> apollo_storage::StorageResult<Option<StateRoots>>;
}

pub(crate) trait BlockHashManagerStorageWriterTrait {
    fn set_block_hash(
        &mut self,
        height: BlockNumber,
        block_hash: BlockHash,
    ) -> apollo_storage::StorageResult<()>;

    fn set_state_roots_hash(
        &mut self,
        height: BlockNumber,
        state_roots: StateRoots,
    ) -> apollo_storage::StorageResult<()>;

    fn set_block_hash_offset(
        &mut self,
        height: BlockNumber,
        block_hash_offset: BlockNumber,
    ) -> apollo_storage::StorageResult<()>;

    fn increment_block_hash_offset(&mut self) -> apollo_storage::StorageResult<()>;

    fn revert_block_hash(&mut self, height: &BlockNumber) -> apollo_storage::StorageResult<()>;

    fn revert_state_roots(&mut self, height: &BlockNumber) -> apollo_storage::StorageResult<()>;
}
