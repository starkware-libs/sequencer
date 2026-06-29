//! JSON-RPC trait implementation for the proving service.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use jsonrpsee::core::RpcResult;
use jsonrpsee::types::ErrorObjectOwned;
use starknet_api::rpc_transaction::RpcTransaction;
use tokio::sync::Semaphore;
use tokio::time::timeout;
use tracing::warn;

use crate::proving::virtual_snos_prover::{ProveTransactionResult, RpcVirtualSnosProver};
use crate::server::config::ServiceConfig;
use crate::server::errors::{internal_server_error, service_busy};
use crate::server::rpc_api::ProvingRpcServer;

/// Starknet RPC specification version.
pub(crate) const SPEC_VERSION: &str = "0.10.2";

/// Implementation of the ProvingRpc trait.
#[derive(Clone)]
pub struct ProvingRpcServerImpl {
    prover: RpcVirtualSnosProver,
    /// Worker slots: how many requests prove concurrently.
    concurrency_semaphore: Arc<Semaphore>,
    /// Total in-flight cap (running + waiting); sized `max_concurrent + max_queued`.
    admission_semaphore: Arc<Semaphore>,
    /// Configured max concurrent requests (used in error messages).
    max_concurrent_requests: usize,
    /// Backstop on the FIFO wait so a stuck worker can't pin a waiter's connection indefinitely.
    queue_wait_timeout: Duration,
}

impl ProvingRpcServerImpl {
    /// Creates a new ProvingRpcServerImpl from a prover.
    pub(crate) fn new(
        prover: RpcVirtualSnosProver,
        max_concurrent_requests: usize,
        max_queued_requests: usize,
        queue_wait_timeout: Duration,
    ) -> Self {
        Self {
            prover,
            concurrency_semaphore: Arc::new(Semaphore::new(max_concurrent_requests)),
            admission_semaphore: Arc::new(Semaphore::new(
                max_concurrent_requests + max_queued_requests,
            )),
            max_concurrent_requests,
            queue_wait_timeout,
        }
    }

    /// Creates a new ProvingRpcServerImpl from configuration.
    pub fn from_config(config: &ServiceConfig) -> Self {
        let prover = RpcVirtualSnosProver::new(&config.prover_config);
        Self::new(
            prover,
            config.max_concurrent_requests,
            config.max_queued_requests,
            Duration::from_millis(config.queue_wait_timeout_millis),
        )
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
        // Admission: cap queue length (running + waiting). Reject with -32005 only when the queue
        // is full; held for the whole request, so a client disconnect frees the slot.
        let _admission = self.admission_semaphore.try_acquire().map_err(|_| {
            warn!(
                max_concurrent_requests = self.max_concurrent_requests,
                "Rejected proving request: queue is full"
            );
            service_busy(self.max_concurrent_requests)
        })?;

        // Wait FIFO for a worker slot (tokio's Semaphore is fair), with queue_wait_timeout as a
        // backstop. Served in arrival order, or cancelled if the client disconnects.
        let _permit = match timeout(self.queue_wait_timeout, self.concurrency_semaphore.acquire())
            .await
        {
            Ok(Ok(permit)) => permit,
            Ok(Err(_)) => return Err(internal_server_error("proving service is shutting down")),
            Err(_) => {
                warn!(
                    max_concurrent_requests = self.max_concurrent_requests,
                    "Rejected proving request: timed out waiting for a worker slot"
                );
                return Err(service_busy(self.max_concurrent_requests));
            }
        };

        self.prover.prove_transaction(block_id, transaction).await.map_err(|err| {
            warn!("prove_transaction failed: {:?}", err);
            ErrorObjectOwned::from(err)
        })
    }
}
