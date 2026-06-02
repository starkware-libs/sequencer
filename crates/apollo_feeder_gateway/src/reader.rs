use std::sync::Arc;

use apollo_feeder_gateway_config::config::FeederGatewayConfig;
use async_trait::async_trait;
use starknet_api::block::BlockHeader;

use crate::errors::FeederGatewayError;

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
}

/// Shared state handed to every axum handler via `Extension`.
#[derive(Clone)]
pub struct AppState {
    pub reader: Arc<dyn ChainDataReader>,
    pub config: FeederGatewayConfig,
}
