//! Utilities for recording and replaying RPC responses in tests.
//!
//! This module provides infrastructure for running integration tests offline
//! by recording JSON-RPC interactions with real nodes and replaying them
//! through a mock HTTP server.
//!
//! ## Modes
//!
//! - **Recording mode** (`RECORD_RPC_RECORDS=1`): Tests run against a real RPC node through a
//!   recording proxy that saves all request/response pairs to JSON files.
//!
//! - **Replay mode** (record files present): Tests start a mock HTTP server that serves
//!   pre-recorded responses, enabling fully offline execution (used in CI).
//!
//! - **Live mode** (default): Tests use a real RPC node directly (existing behavior).

use std::fs;
use std::path::{Path, PathBuf};

use apollo_infra_utils::compile_time_cargo_manifest_dir;
use serde::{Deserialize, Serialize};

/// A recorded JSON-RPC request-response pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcInteraction {
    /// The JSON-RPC method name (e.g., "starknet_getStorageAt").
    pub method: String,
    /// The JSON-RPC parameters.
    pub params: serde_json::Value,
    /// The full JSON-RPC response body.
    pub response: serde_json::Value,
}

/// Collection of recorded RPC interactions for a test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRecords {
    /// All recorded interactions, in order.
    pub interactions: Vec<RpcInteraction>,
}

impl RpcRecords {
    /// Loads recorded RPC interactions from a JSON file.
    pub fn load(path: &Path) -> Self {
        let content = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("Failed to read records from {path:?}: {e}"));
        serde_json::from_str(&content)
            .unwrap_or_else(|e| panic!("Failed to parse records from {path:?}: {e}"))
    }

    /// Saves recorded RPC interactions to a JSON file.
    pub fn save(&self, path: &Path) {
        let dir = path.parent().expect("Invalid record path");
        fs::create_dir_all(dir)
            .unwrap_or_else(|e| panic!("Failed to create directory {dir:?}: {e}"));
        let content =
            serde_json::to_string_pretty(self).expect("Failed to serialize RPC records");
        fs::write(path, content)
            .unwrap_or_else(|e| panic!("Failed to write records to {path:?}: {e}"));
    }
}

/// Creates a mockito server pre-configured with all recorded RPC interactions.
///
/// The server matches JSON-RPC requests by their `method` and `params` fields,
/// returning the recorded response for each matching request.
/// The `id` and `jsonrpc` version fields are ignored during matching so that
/// the mock works with both `RpcStateReader` and `JsonRpcClient` regardless
/// of their internal request formatting.
pub async fn setup_mock_rpc_server(records: &RpcRecords) -> mockito::ServerGuard {
    let mut server = mockito::Server::new_async().await;
    for interaction in &records.interactions {
        let request_matcher = serde_json::json!({
            "method": interaction.method,
            "params": interaction.params,
        });
        server
            .mock("POST", "/")
            .match_body(mockito::Matcher::PartialJson(request_matcher))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&interaction.response).unwrap())
            .create_async()
            .await;
    }
    server
}

// ================================================================================================
// Path helpers
// ================================================================================================

/// Returns the path to the RPC records directory for the starknet_os_runner crate.
pub fn records_dir() -> PathBuf {
    PathBuf::from(compile_time_cargo_manifest_dir!()).join("resources").join("fixtures")
}

/// Returns the path to a specific test's record file.
pub fn record_path(test_name: &str) -> PathBuf {
    records_dir().join(format!("{test_name}.json"))
}

/// Returns true if a record file exists for the given test.
pub fn records_exist(test_name: &str) -> bool {
    record_path(test_name).exists()
}
