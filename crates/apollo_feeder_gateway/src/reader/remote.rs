use apollo_state_sync_types::communication::{SharedStateSyncClient, StateSyncClientError};
use apollo_state_sync_types::errors::StateSyncError;
use async_trait::async_trait;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockSignature};

use crate::errors::FeederGatewayError;
use crate::reader::{internal_error, ChainDataReader, FgResult};

#[cfg(test)]
#[path = "remote_test.rs"]
mod remote_test;

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

    async fn block_hash(&self, block_number: BlockNumber) -> FgResult<BlockHash> {
        self.client.get_block_hash(block_number).await.map_err(map_client_error)
    }

    async fn block_signature(
        &self,
        block_number: BlockNumber,
    ) -> FgResult<(BlockHash, BlockSignature)> {
        let block_hash =
            self.client.get_block_hash(block_number).await.map_err(map_client_error)?;
        let signature =
            self.client.get_block_signature(block_number).await.map_err(map_client_error)?;
        Ok((block_hash, signature))
    }

    async fn block_number_by_hash(&self, block_hash: BlockHash) -> FgResult<Option<BlockNumber>> {
        self.client.get_block_number_by_hash(block_hash).await.map_err(internal_error)
    }
}

/// Maps a state-sync client error to a feeder gateway error, preserving the not-found case (so it
/// becomes the legacy BlockNotFound envelope, HTTP 400) and treating everything else as internal.
fn map_client_error(error: StateSyncClientError) -> FeederGatewayError {
    match error {
        StateSyncClientError::StateSyncError(StateSyncError::BlockNotFound(block_number)) => {
            FeederGatewayError::BlockNotFound(block_number)
        }
        other => internal_error(other),
    }
}
