//! Mock implementation of the proving RPC server for spec conformance tests.
//!
//! [`MockProvingRpc`] returns a canned [`ProveTransactionResult`] loaded from a reference JSON
//! fixture so that [`rpc_spec_test`](super::rpc_spec_test) can validate request/response schemas
//! against the OpenRPC spec without invoking a real prover.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use jsonrpsee::core::RpcResult;
use serde::de::DeserializeOwned;
use starknet_api::rpc_transaction::RpcTransaction;

use crate::proving::virtual_snos_prover::ProveTransactionResult;
use crate::server::rpc_api::ProvingRpcServer;
use crate::server::rpc_impl::SPEC_VERSION;

/// Mock RPC server that returns a pre-loaded [`ProveTransactionResult`].
pub struct MockProvingRpc {
    response: ProveTransactionResult,
}

impl MockProvingRpc {
    /// Loads the pre-loaded response from the reference JSON file.
    pub fn from_expected_json() -> Self {
        Self { response: load_json(&reference_json_dir().join("prove_transaction_result.json")) }
    }
}

#[async_trait]
impl ProvingRpcServer for MockProvingRpc {
    async fn spec_version(&self) -> RpcResult<String> {
        Ok(SPEC_VERSION.to_string())
    }

    async fn prove_transaction(
        &self,
        _block_id: BlockId,
        _transaction: RpcTransaction,
    ) -> RpcResult<ProveTransactionResult> {
        Ok(self.response.clone())
    }
}

/// Reads and deserializes a JSON file into `T`.
fn load_json<T: DeserializeOwned>(path: &Path) -> T {
    let content =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("Failed to read {path:?}: {e}"));
    serde_json::from_str(&content).unwrap_or_else(|e| panic!("Failed to parse {path:?}: {e}"))
}

fn reference_json_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources").join("mock_proving_rpc")
}
