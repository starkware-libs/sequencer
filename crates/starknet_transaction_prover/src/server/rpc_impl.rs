//! JSON-RPC trait implementation for the proving service.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use jsonrpsee::core::RpcResult;
use jsonrpsee::types::ErrorObjectOwned;
use starknet_api::rpc_transaction::RpcTransaction;
use tokio::sync::{Semaphore, SemaphorePermit};
use tokio::time::timeout;
use tracing::warn;

use crate::proving::virtual_snos_prover::{ProveTransactionResult, RpcVirtualSnosProver};
use crate::server::config::ServiceConfig;
use crate::server::errors::{internal_server_error, service_busy};
use crate::server::metrics::{names as metric_names, outcomes, GaugeGuard};
use crate::server::rpc_api::ProvingRpcServer;
use crate::server::saturation::SaturationMonitor;

// `dummy_prover()` builds an `RpcVirtualSnosProver`, which prepares recursive-prover precomputes
// under `stwo_proving`; the reject paths under test are feature-independent, so gate the module to
// the non-proving config to keep it fast.
#[cfg(all(test, not(feature = "stwo_proving")))]
#[path = "rpc_impl_test.rs"]
mod rpc_impl_test;

/// Starknet RPC specification version (matches the pinned `starknet_specs_rev`).
pub(crate) const SPEC_VERSION: &str = "0.10.3-rc.2";

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
    /// Tracks how long the service has been continuously rejecting requests, for health
    /// reporting to query.
    saturation_monitor: SaturationMonitor,
}

impl ProvingRpcServerImpl {
    /// Creates a new ProvingRpcServerImpl from a prover.
    pub(crate) fn new(
        prover: RpcVirtualSnosProver,
        max_concurrent_requests: usize,
        max_queued_requests: usize,
        queue_wait_timeout: Duration,
        saturation_monitor: SaturationMonitor,
    ) -> Self {
        Self {
            prover,
            concurrency_semaphore: Arc::new(Semaphore::new(max_concurrent_requests)),
            admission_semaphore: Arc::new(Semaphore::new(
                max_concurrent_requests + max_queued_requests,
            )),
            max_concurrent_requests,
            queue_wait_timeout,
            saturation_monitor,
        }
    }

    /// Creates a new ProvingRpcServerImpl from configuration.
    pub fn from_config(config: &ServiceConfig, saturation_monitor: SaturationMonitor) -> Self {
        let prover = RpcVirtualSnosProver::new(&config.prover_config);
        Self::new(
            prover,
            config.max_concurrent_requests,
            config.max_queued_requests,
            Duration::from_millis(config.queue_wait_timeout_millis),
            saturation_monitor,
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
        let _admission = self
            .admission_semaphore
            .try_acquire()
            .map_err(|_| self.record_busy_reject(outcomes::REJECTED_QUEUE_FULL, "queue is full"))?;

        // Binding order is load-bearing: `_permit` (bound last) drops first, releasing the worker
        // slot before `_saturation_clear_guard` clears the window; the reverse order would let a
        // rejection latch between clear and release with no future clear.
        let (_saturation_clear_guard, _permit) = self.acquire_worker_slot().await?;

        self.prover.prove_transaction(block_id, transaction).await.map_err(|err| {
            // Not a duplicate of the origin-level breadcrumbs: those name the step that
            // failed, this is the single per-request record of the final outcome.
            // `outcome` is the metric's bounded label set, safe to log. The message
            // itself only goes out when it cannot carry client transaction data --
            // see `may_embed_transaction_data`; these logs leave the service.
            let outcome = err.metric_outcome();
            if err.may_embed_transaction_data() {
                warn!(event = "prove_transaction_failed", outcome, "prove_transaction failed");
            } else {
                warn!(
                    event = "prove_transaction_failed",
                    outcome,
                    error = %err,
                    "prove_transaction failed",
                );
            }
            ErrorObjectOwned::from(err)
        })
    }
}

impl ProvingRpcServerImpl {
    /// Waits for a worker slot. Served in arrival order, or cancelled if the client disconnects.
    /// Tracks the queue depth for the duration of the wait, and records how long the wait took
    /// once a slot is won.
    ///
    /// Returns the permit alongside a `SaturationClearGuard`; the caller must keep both alive for
    /// the proving run.
    async fn acquire_worker_slot(
        &self,
    ) -> Result<(SaturationClearGuard, SemaphorePermit<'_>), ErrorObjectOwned> {
        let wait_start = Instant::now();
        let _waiting_guard = GaugeGuard::acquire(metric_names::QUEUE_WAITING_REQUESTS);
        match timeout(self.queue_wait_timeout, self.concurrency_semaphore.acquire()).await {
            Ok(Ok(permit)) => {
                metrics::histogram!(metric_names::QUEUE_WAIT_DURATION_SECONDS)
                    .record(wait_start.elapsed().as_secs_f64());
                // Clear on acquisition, not only on the guard's drop: a long proving run would
                // otherwise keep reporting saturation that has already ended.
                self.saturation_monitor.mark_progress();
                Ok((
                    SaturationClearGuard { saturation_monitor: self.saturation_monitor.clone() },
                    permit,
                ))
            }
            Ok(Err(_)) => Err(internal_server_error("proving service is shutting down")),
            Err(_) => Err(self.record_busy_reject(
                outcomes::REJECTED_WAIT_TIMEOUT,
                "timed out waiting for a worker slot",
            )),
        }
    }

    /// Records a busy reject: counts it under `outcome` (so served and rejected
    /// requests share one denominator), latches the saturation window, logs it,
    /// and returns the `-32005` error. Kept in one place because the four steps
    /// have to stay together -- a reject that counts but never latches the window
    /// would under-report sustained overload.
    fn record_busy_reject(&self, outcome: &'static str, reason: &str) -> ErrorObjectOwned {
        metrics::counter!(metric_names::PROVE_TRANSACTION_OUTCOME_TOTAL, "outcome" => outcome)
            .increment(1);
        self.saturation_monitor.mark_rejected();
        warn!(
            max_concurrent_requests = self.max_concurrent_requests,
            outcome, "Rejected proving request: {reason}"
        );
        service_busy(self.max_concurrent_requests)
    }
}

/// Clears the saturation window on worker-slot release. Drop-based so the proving-success,
/// proving-error, and client-disconnect exits all clear it.
struct SaturationClearGuard {
    saturation_monitor: SaturationMonitor,
}

impl Drop for SaturationClearGuard {
    fn drop(&mut self) {
        self.saturation_monitor.mark_progress();
    }
}
