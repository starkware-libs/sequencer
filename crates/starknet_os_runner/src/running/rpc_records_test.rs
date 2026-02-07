//! Tests for the RPC records infrastructure.

use rstest::rstest;
use serde_json::json;

use crate::running::rpc_records::{normalize_json, MockRpcServer, RpcInteraction, RpcRecords};

#[rstest]
#[case::sorts_string_array(json!(["c", "a", "b"]), json!(["a", "b", "c"]))]
#[case::sorts_numeric_array(json!([3, 1, 2]), json!([1, 2, 3]))]
#[case::sorts_nested_arrays(
    json!({"outer": ["z", "a"], "nested": {"inner": [3, 1, 2]}}),
    json!({"outer": ["a", "z"], "nested": {"inner": [1, 2, 3]}})
)]
#[case::preserves_primitive(json!(42), json!(42))]
fn test_normalize_json(#[case] input: serde_json::Value, #[case] expected: serde_json::Value) {
    assert_eq!(normalize_json(&input), expected);
}

#[test]
fn test_rpc_records_save_and_load() {
    let records = RpcRecords {
        interactions: vec![RpcInteraction {
            method: "starknet_getNonce".to_string(),
            sorted_params: serde_json::json!({"block_id": "latest", "contract_address": "0x1"}),
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
            sorted_params: serde_json::json!([]),
            response: serde_json::json!({
                "jsonrpc": "2.0",
                "id": 0,
                "result": 42
            }),
        }],
    };

    let server = MockRpcServer::new(&records).await;

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
