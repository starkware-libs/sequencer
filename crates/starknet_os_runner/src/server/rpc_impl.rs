//! JSON-RPC trait implementation for the proving service.

use std::sync::LazyLock;

use async_trait::async_trait;
use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use jsonrpsee::core::RpcResult;
use jsonrpsee::types::ErrorObjectOwned;
use starknet_api::rpc_transaction::RpcTransaction;

use crate::server::config::ServiceConfig;
use crate::server::rpc_trait::{DiscoveryRpcServer, ProvingRpcServer};
use crate::virtual_snos_prover::{ProveTransactionResult, RpcVirtualSnosProver};

/// Starknet RPC specification version.
const SPEC_VERSION: &str = "0.10.0";

/// OpenRPC schema document, embedded at compile time.
const OPENRPC_SCHEMA_STR: &str = include_str!("../../resources/openrpc.json");

/// Parsed OpenRPC schema document.
static OPENRPC_SCHEMA: LazyLock<serde_json::Value> = LazyLock::new(|| {
    serde_json::from_str(OPENRPC_SCHEMA_STR).expect(
        "OpenRPC schema embedded at compile time is invalid JSON - this is a build-time bug",
    )
});

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
        let output = self
            .prover
            .prove_transaction(block_id, transaction)
            .await
            .map_err(ErrorObjectOwned::from)?;

        Ok(output.result)
    }
}

#[async_trait]
impl DiscoveryRpcServer for ProvingRpcServerImpl {
    async fn discover(&self) -> RpcResult<serde_json::Value> {
        Ok(OPENRPC_SCHEMA.clone())
    }
}
