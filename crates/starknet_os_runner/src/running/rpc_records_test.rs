//! Tests for the RPC records infrastructure.

use crate::running::rpc_records::{
    setup_mock_rpc_server,
    RecordingProxy,
    RpcInteraction,
    RpcRecords,
};

#[test]
fn test_rpc_records_round_trip_serialization() {
    let records = RpcRecords {
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

    let serialized = serde_json::to_string_pretty(&records).unwrap();
    let deserialized: RpcRecords = serde_json::from_str(&serialized).unwrap();

    assert_eq!(records.interactions.len(), deserialized.interactions.len());
    assert_eq!(records.interactions[0].method, deserialized.interactions[0].method);
    assert_eq!(records.interactions[0].params, deserialized.interactions[0].params);
    assert_eq!(records.interactions[0].response, deserialized.interactions[0].response);
}

#[test]
fn test_rpc_records_save_and_load() {
    let records = RpcRecords {
        interactions: vec![RpcInteraction {
            method: "starknet_getNonce".to_string(),
            params: serde_json::json!({"block_id": "latest", "contract_address": "0x1"}),
            response: serde_json::json!({"jsonrpc": "2.0", "id": 0, "result": "0x0"}),
        }],
    };

    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("test_record.json");

    records.save(&path);
    let loaded = RpcRecords::load(&path);

    assert_eq!(loaded.interactions.len(), 1);
    assert_eq!(loaded.interactions[0].method, "starknet_getNonce");
}

#[tokio::test]
async fn test_mock_server_matches_rpc_request() {
    let records = RpcRecords {
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

    let server = setup_mock_rpc_server(&records).await;

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

#[tokio::test]
async fn test_recording_proxy_captures_interactions() {
    // Start a mockito server as the "real" RPC backend.
    let mut backend = mockito::Server::new_async().await;
    backend
        .mock("POST", "/")
        .match_body(mockito::Matcher::PartialJson(serde_json::json!({
            "method": "starknet_blockNumber"
        })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"jsonrpc":"2.0","id":0,"result":12345}"#)
        .create_async()
        .await;

    // Start the recording proxy, forwarding to the mock backend.
    let proxy = RecordingProxy::start(&backend.url()).await;
    let proxy_url = proxy.url.clone();

    // Send a request through the proxy.
    let client = reqwest::Client::new();
    let response = client
        .post(&proxy_url)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "starknet_blockNumber",
            "params": []
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["result"], 12345);

    // Collect recorded data.
    let records = proxy.into_records();
    assert_eq!(records.interactions.len(), 1);
    assert_eq!(records.interactions[0].method, "starknet_blockNumber");
    assert_eq!(records.interactions[0].response["result"], 12345);
}

/// End-to-end test: record interactions through proxy, save to file, load, and replay.
#[tokio::test]
async fn test_record_save_load_replay_round_trip() {
    // 1. Set up a mock backend.
    let mut backend = mockito::Server::new_async().await;
    backend
        .mock("POST", "/")
        .match_body(mockito::Matcher::PartialJson(serde_json::json!({
            "method": "starknet_getNonce"
        })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"jsonrpc":"2.0","id":0,"result":"0x42"}"#)
        .create_async()
        .await;

    // 2. Record through proxy.
    let proxy = RecordingProxy::start(&backend.url()).await;
    let client = reqwest::Client::new();
    client
        .post(&proxy.url)
        .json(&serde_json::json!({
            "jsonrpc": "0.7",
            "id": 0,
            "method": "starknet_getNonce",
            "params": {"block_id": "latest", "contract_address": "0x1"}
        }))
        .send()
        .await
        .unwrap();

    let records = proxy.into_records();

    // 3. Save and reload.
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("recorded.json");
    records.save(&path);
    let loaded = RpcRecords::load(&path);

    // 4. Replay using mock server.
    let mock_server = setup_mock_rpc_server(&loaded).await;
    let response = client
        .post(mock_server.url())
        .json(&serde_json::json!({
            "jsonrpc": "0.7",
            "id": 0,
            "method": "starknet_getNonce",
            "params": {"block_id": "latest", "contract_address": "0x1"}
        }))
        .send()
        .await
        .unwrap();

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["result"], "0x42");
}
