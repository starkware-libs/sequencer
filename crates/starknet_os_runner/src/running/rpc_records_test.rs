//! Tests for the RPC records infrastructure.

use blockifier::blockifier::config::ContractClassManagerConfig;
use blockifier::state::contract_class_manager::ContractClassManager;
use starknet_api::core::ChainId;
use url::Url;

use crate::running::rpc_records::{
    finalize_recording,
    setup_mock_rpc_server,
    setup_test_rpc,
    RpcInteraction,
    RpcRecords,
};
use crate::running::runner::{RpcRunnerFactory, RunnerConfig, VirtualSnosRunner};
use crate::running::storage_proofs::StorageProofConfig;
use crate::running::test_utils::{fetch_block_number_from, strk_balance_of_invoke};

#[test]
fn test_rpc_records_round_trip_serialization() {
    let records = RpcRecords {
        interactions: vec![
            RpcInteraction {
                method: "starknet_getStorageAt".to_string(),
                sorted_params: serde_json::json!({
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
                sorted_params: serde_json::json!([]),
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
    assert_eq!(records.interactions[0].sorted_params, deserialized.interactions[0].sorted_params);
    assert_eq!(records.interactions[0].response, deserialized.interactions[0].response);
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

/// Integration test: run balance_of through recording proxy or mock replay.
///
/// # Modes
///
/// - **Record**: `RECORD_RPC_RECORDS=1 SEPOLIA_NODE_URL=... cargo test -p starknet_os_runner
///   test_balance_of_record_replay -- --ignored`
/// - **Replay**: `cargo test -p starknet_os_runner test_balance_of_record_replay -- --ignored`
///   (requires `resources/fixtures/balance_of.json`)
#[tokio::test(flavor = "multi_thread")]
#[ignore] // Requires RPC access or pre-recorded fixtures.
async fn test_balance_of_record_replay() {
    let test_name = "balance_of";
    let (rpc_url, proxy, _server) = setup_test_rpc(test_name).await;

    let rpc_url_parsed = Url::parse(&rpc_url).expect("Invalid RPC URL");
    let contract_class_manager = ContractClassManager::start(ContractClassManagerConfig::default());
    let config =
        RunnerConfig { storage_proof_config: StorageProofConfig { include_state_changes: true } };
    let factory =
        RpcRunnerFactory::new(rpc_url_parsed, ChainId::Sepolia, contract_class_manager, config);

    let block_id = fetch_block_number_from(&rpc_url).await;
    let tx = strk_balance_of_invoke();

    factory
        .run_virtual_os(block_id, vec![tx])
        .await
        .expect("run_virtual_os should succeed");

    finalize_recording(proxy, test_name);
}
