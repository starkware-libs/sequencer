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
use crate::server::errors::service_busy;
use crate::server::metrics::names::CONCURRENCY_REJECTED_TOTAL;
use crate::server::rpc_api::ProvingRpcServer;
use crate::server::saturation::SaturationMonitor;

/// Starknet RPC specification version.
pub(crate) const SPEC_VERSION: &str = "0.10.1";

/// Implementation of the ProvingRpc trait.
#[derive(Clone)]
pub struct ProvingRpcServerImpl {
    prover: RpcVirtualSnosProver,
    /// Limits how many proving requests can run concurrently.
    concurrency_semaphore: Arc<Semaphore>,
    /// Configured max concurrent requests (used in error messages).
    max_concurrent_requests: usize,
    /// Tracks how long the service has been rejecting requests so
    /// `/health` can flip to 503. Cloned cheaply (Arc internally).
    saturation_monitor: SaturationMonitor,
}

impl ProvingRpcServerImpl {
    /// Creates a new ProvingRpcServerImpl from a prover.
    pub(crate) fn new(
        prover: RpcVirtualSnosProver,
        max_concurrent_requests: usize,
        saturation_monitor: SaturationMonitor,
    ) -> Self {
        Self {
            prover,
            concurrency_semaphore: Arc::new(Semaphore::new(max_concurrent_requests)),
            max_concurrent_requests,
            saturation_monitor,
        }
    }

    /// Creates a new ProvingRpcServerImpl from configuration.
    pub fn from_config(config: &ServiceConfig, saturation_monitor: SaturationMonitor) -> Self {
        let prover = RpcVirtualSnosProver::new(&config.prover_config);
        Self::new(prover, config.max_concurrent_requests, saturation_monitor)
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
            metrics::counter!(CONCURRENCY_REJECTED_TOTAL).increment(1);
            self.saturation_monitor.mark_rejected();
            warn!(
                max_concurrent_requests = self.max_concurrent_requests,
                "Rejected proving request: service is at capacity"
            );
            service_busy(self.max_concurrent_requests)
        })?;
        // We hold the permit. The service successfully accepted this
        // request — clear any saturation window so /health can recover.
        self.saturation_monitor.mark_accepted();

        self.prover.prove_transaction(block_id, transaction).await.map_err(|err| {
            // Structured fields match `prover_prove_transaction_outcome_total`
            // so log queries and metric filters use the same vocabulary. The
            // origin-level warns inside `virtual_snos_prover` cover the
            // specific failure mode; this one is the catch-all backstop.
            warn!(
                event = "prove_transaction_failed",
                outcome = err.metric_outcome(),
                error = %err,
                "prove_transaction failed",
            );
            ErrorObjectOwned::from(err)
        })
    }
}
