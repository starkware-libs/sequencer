//! JSON-RPC trait implementation for the proving service.

use std::sync::Arc;

use async_trait::async_trait;
use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use jsonrpsee::core::RpcResult;
use jsonrpsee::types::ErrorObjectOwned;
use starknet_api::rpc_transaction::RpcTransaction;
use tokio::sync::Semaphore;
use tracing::warn;

use crate::proving::virtual_snos_prover::{ProveTransactionResult, RpcVirtualSnosProver};
use crate::server::config::ServiceConfig;
use crate::server::error::service_busy;
use crate::server::rpc_trait::ProvingRpcServer;

/// Starknet RPC specification version.
const SPEC_VERSION: &str = "0.10.0";

/// Implementation of the ProvingRpc trait.
#[derive(Clone)]
pub struct ProvingRpcServerImpl {
    prover: RpcVirtualSnosProver,
    /// Limits how many proving requests can run concurrently.
    concurrency_semaphore: Arc<Semaphore>,
    /// Configured max concurrent requests (used in error messages).
    max_concurrent_requests: usize,
}

impl ProvingRpcServerImpl {
    /// Creates a new ProvingRpcServerImpl from a prover.
    pub(crate) fn new(prover: RpcVirtualSnosProver, max_concurrent_requests: usize) -> Self {
        Self {
            prover,
            concurrency_semaphore: Arc::new(Semaphore::new(max_concurrent_requests)),
            max_concurrent_requests,
        }
    }

    /// Creates a new ProvingRpcServerImpl from configuration.
    pub fn from_config(config: &ServiceConfig) -> Self {
        let prover = RpcVirtualSnosProver::new(&config.prover_config);
        Self::new(prover, config.max_concurrent_requests)
    }
}

#[async_trait]
impl ProvingRpcServer for ProvingRpcServerImpl {
    async fn spec_version(&self) -> RpcResult<String> {
        Ok(SPEC_VERSION.to_string())
    }

    async fn prove_transaction(
        &self,
        block_id: BlockId,
        transaction: RpcTransaction,
    ) -> RpcResult<ProveTransactionResult> {
        let _permit = self.concurrency_semaphore.try_acquire().map_err(|_| {
            warn!(
                max_concurrent_requests = self.max_concurrent_requests,
                "Rejected proving request: service is at capacity"
            );
            service_busy(self.max_concurrent_requests)
        })?;

        let output = self
            .prover
            .prove_transaction(block_id, transaction)
            .await
            .map_err(ErrorObjectOwned::from)?;

        Ok(output.result)
    }
}
