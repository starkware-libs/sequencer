//! JSON-RPC trait implementation for the proving service.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use jsonrpsee::core::RpcResult;
use jsonrpsee::types::ErrorObjectOwned;
use starknet_api::rpc_transaction::RpcTransaction;
use tokio::sync::Semaphore;
use tracing::warn;

use crate::metrics::ProvingMetrics;
use crate::proving::virtual_snos_prover::{
    ProveTransactionResult,
    RpcVirtualSnosProver,
    VirtualSnosProverError,
};
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
    /// OpenTelemetry metrics instruments.
    metrics: ProvingMetrics,
}

impl ProvingRpcServerImpl {
    /// Creates a new ProvingRpcServerImpl from a prover.
    pub(crate) fn new(
        prover: RpcVirtualSnosProver,
        max_concurrent_requests: usize,
        metrics: ProvingMetrics,
    ) -> Self {
        Self {
            prover,
            concurrency_semaphore: Arc::new(Semaphore::new(max_concurrent_requests)),
            max_concurrent_requests,
            metrics,
        }
    }

    /// Creates a new ProvingRpcServerImpl from configuration and metrics.
    pub fn from_config(config: &ServiceConfig, metrics: ProvingMetrics) -> Self {
        let prover = RpcVirtualSnosProver::new(&config.prover_config);
        Self::new(prover, config.max_concurrent_requests, metrics)
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
        self.metrics.record_request_received();
        let request_start = Instant::now();

        let _permit = self.concurrency_semaphore.try_acquire().map_err(|_| {
            warn!(
                max_concurrent_requests = self.max_concurrent_requests,
                "Rejected proving request: service is at capacity"
            );
            self.metrics.record_request_rejected();
            service_busy(self.max_concurrent_requests)
        })?;

        let output = self
            .prover
            .prove_transaction(block_id, transaction)
            .await
            .map_err(|err| {
                let error_type = match &err {
                    VirtualSnosProverError::InvalidTransactionType(_) => "invalid_tx_type",
                    VirtualSnosProverError::ValidationError(_) => "validation",
                    VirtualSnosProverError::RunnerError(_) => "runner",
                    VirtualSnosProverError::ProvingError(_) => "proving",
                    VirtualSnosProverError::OutputParseError(_) => "output_parse",
                    VirtualSnosProverError::ProgramOutputError(_) => "program_output",
                };
                self.metrics.record_request_failed(error_type);
                ErrorObjectOwned::from(err)
            })?;

        self.metrics.record_os_execution_duration(output.os_duration.as_secs_f64());
        self.metrics.record_proving_duration(output.prove_duration.as_secs_f64());
        self.metrics.record_os_execution_steps(output.n_steps);
        self.metrics.record_request_succeeded(request_start.elapsed().as_secs_f64());

        Ok(output.result)
    }
}
