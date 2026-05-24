//! JSON-RPC trait implementation for the proving service.

use std::sync::Arc;
use std::time::{Duration, Instant};

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
use crate::server::metrics::{names as metric_names, outcomes};
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
    /// Tracks how long the service has been rejecting requests so
    /// `/health` can flip to 503. Cloned cheaply (Arc internally).
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
        // is full; held for the whole request, so a client disconnect frees the slot. Busy-rejects
        // are folded into the outcome counter (as a `rejected_*` outcome) so every request —
        // served or shed — shares one denominator for rate calculations.
        let _admission = self.admission_semaphore.try_acquire().map_err(|_| {
            metrics::counter!(
                metric_names::PROVE_TRANSACTION_OUTCOME_TOTAL,
                "outcome" => outcomes::REJECTED_QUEUE_FULL,
            )
            .increment(1);
            self.saturation_monitor.mark_rejected();
            warn!(
                max_concurrent_requests = self.max_concurrent_requests,
                "Rejected proving request: queue is full"
            );
            service_busy(self.max_concurrent_requests)
        })?;

        // Wait FIFO for a worker slot (tokio's Semaphore is fair), with queue_wait_timeout as a
        // backstop. `QueueWaitingGuard` decrements the queue-depth gauge on every exit from the
        // wait — slot acquired, timeout, shutdown, or client disconnect — so the gauge can't leak.
        let wait_start = Instant::now();
        let _permit = {
            metrics::gauge!(metric_names::QUEUE_WAITING_REQUESTS).increment(1.0);
            let _waiting_guard = QueueWaitingGuard;
            match timeout(self.queue_wait_timeout, self.concurrency_semaphore.acquire()).await {
                Ok(Ok(permit)) => {
                    metrics::histogram!(metric_names::QUEUE_WAIT_DURATION_SECONDS)
                        .record(wait_start.elapsed().as_secs_f64());
                    // A worker slot opened up and this request is about to prove — clear any
                    // saturation window so /health can recover.
                    self.saturation_monitor.mark_accepted();
                    permit
                }
                Ok(Err(_)) => {
                    return Err(internal_server_error("proving service is shutting down"));
                }
                Err(_) => {
                    metrics::counter!(
                        metric_names::PROVE_TRANSACTION_OUTCOME_TOTAL,
                        "outcome" => outcomes::REJECTED_WAIT_TIMEOUT,
                    )
                    .increment(1);
                    self.saturation_monitor.mark_rejected();
                    warn!(
                        max_concurrent_requests = self.max_concurrent_requests,
                        "Rejected proving request: timed out waiting for a worker slot"
                    );
                    return Err(service_busy(self.max_concurrent_requests));
                }
            }
        };

        self.prover.prove_transaction(block_id, transaction).await.map_err(|err| {
            // `outcome` matches `prover_prove_transaction_outcome_total` so log queries and metric
            // filters share one vocabulary. Validation failures can embed the client's fee inputs
            // in their message, so omit `error` for them (the origin-level warn in
            // `virtual_snos_prover` already logged the reason); other failures are safe to detail.
            let outcome = err.metric_outcome();
            if outcome == outcomes::VALIDATION {
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

/// Decrements [`metric_names::QUEUE_WAITING_REQUESTS`] on drop. Using a guard rather than an
/// explicit decrement covers the timeout, shutdown, and cancellation (client-disconnect) paths so
/// the gauge always returns to its true depth.
struct QueueWaitingGuard;

impl Drop for QueueWaitingGuard {
    fn drop(&mut self) {
        metrics::gauge!(metric_names::QUEUE_WAITING_REQUESTS).decrement(1.0);
    }
}
