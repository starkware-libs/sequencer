//! JSON-RPC trait implementation for the proving service.

use async_trait::async_trait;
use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use jsonrpsee::core::RpcResult;
use jsonrpsee::types::ErrorObjectOwned;
use starknet_api::rpc_transaction::RpcTransaction;

use crate::server::config::ServiceConfig;
use crate::server::metrics::{
    register_metrics,
    PROVING_OS_EXECUTION_LATENCY,
    PROVING_PROVER_LATENCY,
    PROVING_REQUESTS_FAILURE,
    PROVING_REQUESTS_SUCCESS,
    PROVING_REQUESTS_TOTAL,
    PROVING_TOTAL_LATENCY,
};
use crate::server::rpc_trait::ProvingRpcServer;
use crate::virtual_snos_prover::{ProveTransactionResult, RpcVirtualSnosProver};

/// Starknet RPC specification version.
const SPEC_VERSION: &str = "0.10.0";

/// Implementation of the ProvingRpc trait.
#[derive(Clone)]
pub struct ProvingRpcServerImpl {
    prover: RpcVirtualSnosProver,
}

impl ProvingRpcServerImpl {
    /// Creates a new ProvingRpcServerImpl from a prover.
    pub(crate) fn new(prover: RpcVirtualSnosProver) -> Self {
        Self { prover }
    }

    /// Creates a new ProvingRpcServerImpl from configuration.
    pub fn from_config(config: &ServiceConfig) -> Self {
        let prover = RpcVirtualSnosProver::new(&config.prover_config);
        Self::new(prover)
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
        // Record that we received a request.
        PROVING_REQUESTS_TOTAL.increment(1);

        // Delegate to the prover.
        let result = self.prover.prove_transaction(block_id, transaction).await;

        match result {
            Ok(output) => {
                // Record success metrics.
                PROVING_REQUESTS_SUCCESS.increment(1);
                PROVING_OS_EXECUTION_LATENCY.record(output.os_duration.as_secs_f64());
                PROVING_PROVER_LATENCY.record(output.prove_duration.as_secs_f64());
                PROVING_TOTAL_LATENCY.record(output.total_duration.as_secs_f64());

                // Build response.
                Ok(output.result)
            }
            Err(e) => {
                // Record failure metric.
                PROVING_REQUESTS_FAILURE.increment(1);
                Err(ErrorObjectOwned::from(e))
            }
        }
    }
}
