//! Mock implementation of `prove_transaction` for SDK integration tests.
//!
//! The SDK calls [`MockProvingRpc::prove_transaction`] instead of a real prover.
//!
//! If the response format (`ProveTransactionResult`) or input types (`BlockId`,
//! `RpcTransaction`) change, the SDK will fail to send or parse the response.
//!
//! # Usage
//!
//! ```rust,ignore
//! let mock = MockProvingRpc::from_expected_json();
//! let result = mock.prove_transaction(block_id, transaction);
//! ```

#[cfg(test)]
#[path = "mock_rpc_test.rs"]
mod mock_rpc_test;

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use jsonrpsee::core::RpcResult;
use serde::de::DeserializeOwned;
use starknet_api::rpc_transaction::RpcTransaction;

use crate::proving::virtual_snos_prover::ProveTransactionResult;
use crate::server::rpc_api::ProvingRpcServer;
use crate::server::rpc_impl::SPEC_VERSION;

/// Mock of `prove_transaction` that returns a pre-loaded [`ProveTransactionResult`]
/// loaded from `resources/mock_proving_rpc/prove_transaction_result.json`.
///
/// The JSON values are synthetic (arbitrary but structurally valid); only the **format** matters
/// for SDK compatibility, not the actual content.
pub struct MockProvingRpc {
    response: ProveTransactionResult,
}

impl MockProvingRpc {
    /// Loads the pre-loaded response from the reference JSON file.
    pub fn from_expected_json() -> Self {
        Self { response: load_expected_prove_transaction_result() }
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

/// Loads `prove_transaction_result.json` as a [`ProveTransactionResult`].
pub fn load_expected_prove_transaction_result() -> ProveTransactionResult {
    load_json(&reference_json_dir().join("prove_transaction_result.json"))
}
