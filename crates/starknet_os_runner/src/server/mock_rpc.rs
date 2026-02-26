//! Mock implementation of `prove_transaction` for SDK integration testing.
//!
//! [`MockProvingRpc`] mirrors the real `prove_transaction` interface: it accepts
//! `BlockId` + `RpcTransaction`, validates the input by serializing it and comparing
//! against the reference request JSON, and returns a canned [`ProveTransactionResult`]
//! loaded from a fixture file.
//!
//! # Fixture files
//!
//! Stored under `resources/mock_proving_rpc/`:
//! - `prove_transaction_params.json` — reference request params (block_id + transaction).
//! - `prove_transaction_result.json` — canned [`ProveTransactionResult`] response.
//!
//! # Usage (from the SDK)
//!
//! ```rust,ignore
//! use starknet_os_runner::server::mock_rpc::MockProvingRpc;
//!
//! let mock = MockProvingRpc::from_fixtures();
//! let result = mock.prove_transaction(block_id, transaction);
//! ```

#[cfg(test)]
#[path = "mock_rpc_test.rs"]
mod mock_rpc_test;

use std::path::PathBuf;

use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use starknet_api::rpc_transaction::RpcTransaction;

use crate::proving::virtual_snos_prover::ProveTransactionResult;

// ================================================================================================
// MockProvingRpc
// ================================================================================================

/// Mock implementation of `prove_transaction`.
///
/// Validates that the input serializes to JSON matching the reference fixture, then
/// returns a pre-loaded [`ProveTransactionResult`].
pub struct MockProvingRpc {
    /// Expected request params (block_id + transaction) as JSON.
    expected_params: serde_json::Value,
    /// Canned response to return.
    response: ProveTransactionResult,
}

impl MockProvingRpc {
    /// Creates a mock by loading both fixtures from the default resource directory.
    pub fn from_fixtures() -> Self {
        Self { expected_params: load_fixture_params(), response: load_fixture_response() }
    }

    /// Proves a transaction (mock version).
    ///
    /// Serializes `block_id` and `transaction` to JSON and asserts the structure
    /// matches the reference fixture, then returns the canned response.
    pub fn prove_transaction(
        &self,
        block_id: BlockId,
        transaction: RpcTransaction,
    ) -> ProveTransactionResult {
        // Serialize the inputs to JSON so we can compare against the fixture.
        let actual_block_id =
            serde_json::to_value(&block_id).expect("Failed to serialize block_id");
        let actual_transaction =
            serde_json::to_value(&transaction).expect("Failed to serialize transaction");

        // Validate the input matches the expected fixture format.
        assert_eq!(
            self.expected_params["block_id"], actual_block_id,
            "block_id does not match the expected fixture format"
        );
        assert_eq!(
            self.expected_params["transaction"], actual_transaction,
            "transaction does not match the expected fixture format"
        );

        self.response.clone()
    }
}

// ================================================================================================
// Fixture helpers
// ================================================================================================

/// Returns the path to the mock fixture directory.
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources").join("mock_proving_rpc")
}

/// Loads the request-params fixture as a raw JSON value.
///
/// The fixture contains `{ "block_id": ..., "transaction": ... }`.
pub fn load_fixture_params() -> serde_json::Value {
    let path = fixtures_dir().join("prove_transaction_params.json");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read fixture {path:?}: {e}"));
    serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("Failed to parse fixture {path:?}: {e}"))
}

/// Loads the response fixture and deserializes it into [`ProveTransactionResult`].
pub fn load_fixture_response() -> ProveTransactionResult {
    let path = fixtures_dir().join("prove_transaction_result.json");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read fixture {path:?}: {e}"));
    serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("Failed to parse fixture {path:?}: {e}"))
}
