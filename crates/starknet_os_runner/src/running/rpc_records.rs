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

use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use apollo_infra_utils::compile_time_cargo_manifest_dir;
use axum::body::Bytes;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::net::TcpListener;

/// Address for servers to bind to (port 0 = OS-assigned random port).
const SERVER_BIND_ADDRESS: &str = "127.0.0.1:0";

/// A recorded JSON-RPC request-response pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcInteraction {
    /// The JSON-RPC method name (e.g., "starknet_getStorageAt").
    pub method: String,
    /// The JSON-RPC parameters (normalized: arrays sorted for deterministic matching).
    pub params: Value,
    /// The full JSON-RPC response body.
    pub response: Value,
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
        let content = serde_json::to_string_pretty(self).expect("Failed to serialize RPC records");
        fs::write(path, content)
            .unwrap_or_else(|e| panic!("Failed to write records to {path:?}: {e}"));
    }
}

// ================================================================================================
// JSON normalization
// ================================================================================================

/// Recursively sorts arrays in a JSON value for deterministic comparison.
///
/// Rust collections (`HashSet`, `HashMap`) iterate in non-deterministic order,
/// so RPC params containing arrays (e.g., `class_hashes` in `starknet_getStorageProof`)
/// may differ between runs. Normalizing before save and before lookup ensures matching.
pub fn normalize_json(value: &Value) -> Value {
    match value {
        Value::Array(arr) => {
            let mut items: Vec<Value> = arr.iter().map(normalize_json).collect();
            items.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
            Value::Array(items)
        }
        Value::Object(obj) => {
            Value::Object(obj.iter().map(|(k, v)| (k.clone(), normalize_json(v))).collect())
        }
        other => other.clone(),
    }
}

/// Builds a lookup key from method name and normalized params.
fn make_lookup_key(method: &str, params: &Value) -> String {
    format!("{}:{}", method, normalize_json(params))
}

// ================================================================================================
// Mock RPC Server (for replay mode)
// ================================================================================================

/// A mock RPC server that replays pre-recorded interactions.
pub struct MockRpcServer {
    url: String,
    /// Dropping this signals the mock server to shut down.
    _server_shutdown: tokio::sync::oneshot::Sender<()>,
}

impl MockRpcServer {
    pub fn url(&self) -> String {
        self.url.clone()
    }
}

/// Creates a mock RPC server that replays pre-recorded interactions.
///
/// Matches requests by `method` + normalized `params` (arrays sorted).
pub async fn setup_mock_rpc_server(records: &RpcRecords) -> MockRpcServer {
    let mut lookup: HashMap<String, Value> = HashMap::new();
    for interaction in &records.interactions {
        let key = make_lookup_key(&interaction.method, &interaction.params);
        lookup.insert(key, interaction.response.clone());
    }

    // Every request will be handled by the mock_rpc_handler with the lookup map.
    let state = Arc::new(lookup);
    let app = Router::new().route("/", post(mock_rpc_handler)).with_state(state);

    let listener = TcpListener::bind(SERVER_BIND_ADDRESS).await.unwrap();
    // We need to get the local address to construct the URL for the mock server.
    // We previously used port 0 to get a random port, so now we figure out the port.
    let addr: SocketAddr = listener.local_addr().unwrap();

    // Create a channel to signal the mock server to shut down.
    // The server will shut down when the MockRpcServer is dropped.
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                shutdown_rx.await.ok();
            })
            .await
            .unwrap();
    });

    MockRpcServer { url: format!("http://{addr}"), _server_shutdown: shutdown_tx }
}

/// Handles a JSON-RPC request by looking up the response in the lookup map.
async fn mock_rpc_handler(
    State(lookup): State<Arc<HashMap<String, Value>>>,
    body: Bytes,
) -> impl IntoResponse {
    let request: Value =
        serde_json::from_slice(&body).expect("Mock RPC server: invalid JSON in request body");
    let method = request["method"].as_str().unwrap_or("unknown");
    let params = request.get("params").cloned().unwrap_or(Value::Null);
    let key = make_lookup_key(method, &params);

    match lookup.get(&key) {
        Some(response) => axum::Json(response.clone()).into_response(),
        None => {
            eprintln!("Mock RPC server: no match for {key}");
            (axum::http::StatusCode::NOT_FOUND, "No matching recorded interaction").into_response()
        }
    }
}

// ================================================================================================
// Path helpers
// ================================================================================================

/// Returns the path to the RPC records directory for the starknet_os_runner crate.
pub fn records_dir() -> PathBuf {
    PathBuf::from(compile_time_cargo_manifest_dir!()).join("resources").join("rpc_records")
}

/// Returns the path to a specific test's record file.
pub fn record_path(test_name: &str) -> PathBuf {
    records_dir().join(format!("{test_name}.json"))
}

/// Returns true if a record file exists for the given test.
pub fn records_exist(test_name: &str) -> bool {
    record_path(test_name).exists()
}
