use apollo_state_sync_types::communication::{SharedStateSyncClient, StateSyncClientError};
use apollo_state_sync_types::errors::StateSyncError;
use async_trait::async_trait;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockSignature};

use crate::errors::FeederGatewayError;
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

    async fn block_hash(&self, block_number: BlockNumber) -> FgResult<BlockHash> {
        self.client.get_block_hash(block_number).await.map_err(map_client_error)
    }

    async fn block_signature(
        &self,
        _block_number: BlockNumber,
    ) -> FgResult<(BlockHash, BlockSignature)> {
        // The state-sync client has no block-signature read yet (it needs a Reference-C extension:
        // a GetBlockSignature request/response variant + handler). The remote backend is
        // default-off (the node defaults to co-located), so this is a documented gap, not a
        // default code path.
        tracing::error!("get_signature is not yet supported on the remote feeder gateway backend");
        Err(FeederGatewayError::Internal)
    }
}

/// Maps a state-sync client error to a feeder gateway error, preserving the not-found case (so it
/// becomes the legacy BlockNotFound envelope, HTTP 400) and treating everything else as internal.
fn map_client_error(error: StateSyncClientError) -> FeederGatewayError {
    match error {
        StateSyncClientError::StateSyncError(StateSyncError::BlockNotFound(_)) => {
            FeederGatewayError::BlockNotFound
        }
        other => internal_error(other),
    }
}
