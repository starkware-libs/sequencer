//! Utilities for recording and replaying RPC responses in tests.
//!
//! This module provides infrastructure for running integration tests offline
//! by recording JSON-RPC interactions with real nodes and replaying them
//! through a mock HTTP server.
//!
//! ## Modes
//!
//! - **Recording mode** (`RECORD_RPC_FIXTURES=1`): Tests run against a real RPC node
//!   through a recording proxy that saves all request/response pairs to JSON fixture files.
//!
//! - **Replay mode** (fixture files present): Tests start a mock HTTP server that serves
//!   pre-recorded responses, enabling fully offline execution (used in CI).
//!
//! - **Live mode** (default): Tests use a real RPC node directly (existing behavior).

use std::fs;
use std::path::Path;

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
pub struct TestFixtures {
    /// All recorded interactions, in order.
    pub interactions: Vec<RpcInteraction>,
}

impl TestFixtures {
    /// Loads test fixtures from a JSON file.
    pub fn load(path: &str) -> Self {
        let content = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("Failed to read fixtures from {path}: {e}"));
        serde_json::from_str(&content)
            .unwrap_or_else(|e| panic!("Failed to parse fixtures from {path}: {e}"))
    }

    /// Saves test fixtures to a JSON file.
    pub fn save(&self, path: &str) {
        let dir = Path::new(path).parent().expect("Invalid fixture path");
        fs::create_dir_all(dir)
            .unwrap_or_else(|e| panic!("Failed to create directory {dir:?}: {e}"));
        let content = serde_json::to_string_pretty(self).expect("Failed to serialize fixtures");
        fs::write(path, content)
            .unwrap_or_else(|e| panic!("Failed to write fixtures to {path}: {e}"));
    }
}

/// Creates a mockito server pre-configured with all recorded RPC interactions.
///
/// The server matches JSON-RPC requests by their `method` and `params` fields,
/// returning the recorded response for each matching request.
/// The `id` and `jsonrpc` version fields are ignored during matching so that
/// the mock works with both `RpcStateReader` and `JsonRpcClient` regardless
/// of their internal request formatting.
pub async fn setup_mock_rpc_server(fixtures: &TestFixtures) -> mockito::ServerGuard {
    let mut server = mockito::Server::new_async().await;
    for interaction in &fixtures.interactions {
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

/// Returns the path to the fixtures directory for the starknet_os_runner crate.
pub fn fixtures_dir() -> String {
    format!("{}/resources/fixtures", env!("CARGO_MANIFEST_DIR"))
}

/// Returns the path to a specific test's fixture file.
pub fn fixture_path(test_name: &str) -> String {
    format!("{}/{test_name}.json", fixtures_dir())
}

/// Returns true if fixture files exist for the given test.
pub fn fixtures_exist(test_name: &str) -> bool {
    Path::new(&fixture_path(test_name)).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixtures_round_trip_serialization() {
        let fixtures = TestFixtures {
            interactions: vec![
                RpcInteraction {
                    method: "starknet_getStorageAt".to_string(),
                    params: serde_json::json!({
                        "block_id": {"block_number": 100},
                        "contract_address": "0x1",
                        "key": "0x2"
                    }),
                    response: serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": 0,
                        "result": "0x999"
                    }),
                },
                RpcInteraction {
                    method: "starknet_blockNumber".to_string(),
                    params: serde_json::json!([]),
                    response: serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "result": 100
                    }),
                },
            ],
        };

        let serialized = serde_json::to_string_pretty(&fixtures).unwrap();
        let deserialized: TestFixtures = serde_json::from_str(&serialized).unwrap();

        assert_eq!(fixtures.interactions.len(), deserialized.interactions.len());
        assert_eq!(fixtures.interactions[0].method, deserialized.interactions[0].method);
        assert_eq!(fixtures.interactions[0].params, deserialized.interactions[0].params);
        assert_eq!(fixtures.interactions[0].response, deserialized.interactions[0].response);
    }

    #[test]
    fn test_fixtures_save_and_load() {
        let fixtures = TestFixtures {
            interactions: vec![RpcInteraction {
                method: "starknet_getNonce".to_string(),
                params: serde_json::json!({"block_id": "latest", "contract_address": "0x1"}),
                response: serde_json::json!({"jsonrpc": "2.0", "id": 0, "result": "0x0"}),
            }],
        };

        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("test_fixture.json");
        let path_str = path.to_str().unwrap();

        fixtures.save(path_str);
        let loaded = TestFixtures::load(path_str);

        assert_eq!(loaded.interactions.len(), 1);
        assert_eq!(loaded.interactions[0].method, "starknet_getNonce");
    }

    #[tokio::test]
    async fn test_mock_server_matches_rpc_request() {
        let fixtures = TestFixtures {
            interactions: vec![RpcInteraction {
                method: "starknet_blockNumber".to_string(),
                params: serde_json::json!([]),
                response: serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 0,
                    "result": 42
                }),
            }],
        };

        let server = setup_mock_rpc_server(&fixtures).await;

        // Send a JSON-RPC request with different id/jsonrpc version (should still match).
        let client = reqwest::Client::new();
        let response = client
            .post(server.url())
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 99,
                "method": "starknet_blockNumber",
                "params": []
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["result"], 42);
    }
}
