use apollo_state_sync_types::communication::SharedStateSyncClient;
use async_trait::async_trait;
use starknet_api::block::BlockHeader;

use crate::reader::{internal_error, ChainDataReader, FgResult};

/// A [`ChainDataReader`] for different-pod/node deployments: it delegates every read to the
/// state-sync process over the network via a [`SharedStateSyncClient`]. The feeder gateway is
/// stateless in this mode and holds no local storage.
pub struct RemoteChainDataReader {
    client: SharedStateSyncClient,
}

impl RemoteChainDataReader {
    pub fn new(client: SharedStateSyncClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl ChainDataReader for RemoteChainDataReader {
    async fn latest_block_header(&self) -> FgResult<Option<BlockHeader>> {
        self.client.get_latest_block_header().await.map_err(internal_error)
    }
}
