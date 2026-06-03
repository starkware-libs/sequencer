use std::sync::Arc;

use apollo_feeder_gateway_config::config::FeederGatewayConfig;
use async_trait::async_trait;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockSignature};

use crate::errors::FeederGatewayError;

pub mod colocated;
pub mod executor;
pub mod remote;

#[cfg(test)]
#[path = "reader_test.rs"]
mod reader_test;

pub type FgResult<T> = Result<T, FeederGatewayError>;

/// The feeder gateway's read backend. Implementations: `ColocatedStorageReader` (direct
/// `StorageReader` access, via the bounded `ReadExecutor`) and `RemoteChainDataReader` (delegating
/// to a `SharedStateSyncClient`). One method per read primitive; widened one endpoint at a time.
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait ChainDataReader: Send + Sync + 'static {
    async fn latest_block_header(&self) -> FgResult<Option<BlockHeader>>;

    /// The block hash of `block_number`, or [`FeederGatewayError::BlockNotFound`] if no such block
    /// has been synced.
    async fn block_hash(&self, block_number: BlockNumber) -> FgResult<BlockHash>;

    /// The block hash and signature of `block_number`, or [`FeederGatewayError::BlockNotFound`] if
    /// no such block (or its signature) has been synced.
    async fn block_signature(
        &self,
        block_number: BlockNumber,
    ) -> FgResult<(BlockHash, BlockSignature)>;
}

/// Shared state handed to every axum handler via `Extension`.
#[derive(Clone)]
pub struct AppState {
    pub reader: Arc<dyn ChainDataReader>,
    pub config: FeederGatewayConfig,
}

/// Maps an internal read/backend error to [`FeederGatewayError::Internal`], logging the source here
/// (the only place it is observed) so no internal detail leaks to the client.
pub(crate) fn internal_error<E: std::fmt::Display>(error: E) -> FeederGatewayError {
    tracing::error!(error = %error, "feeder gateway internal read error");
    FeederGatewayError::Internal
}
