use std::sync::{Arc, Mutex};

use blockifier::state::state_api::StateReader;
use serde::Serialize;
use serde_json::json;
use starknet_api::core::ChainId;
use starknet_api::{class_hash, contract_address, felt, nonce};

use crate::state_reader::config::RpcStateReaderConfig;
use crate::state_reader::rpc_objects::{
    BlockId,
    GetClassHashAtParams,
    GetNonceParams,
    GetStorageAtParams,
    RpcResponse,
    RpcSuccessResponse,
};
use crate::state_reader::rpc_state_reader::{RetryConfig, RpcStateReader};
use crate::utils::get_chain_info;

fn rpc_state_reader_from_latest(config: &RpcStateReaderConfig) -> RpcStateReader {
    RpcStateReader {
        config: config.clone(),
        block_id: BlockId::Latest,
        retry_config: RetryConfig::default(),
        chain_info: get_chain_info(&ChainId::Mainnet, None),
        contract_class_mapping_dumper: Arc::new(Mutex::new(None)),
    }
}

async fn run_rpc_server() -> mockito::ServerGuard {
    mockito::Server::new_async().await
}

fn mock_rpc_interaction(
    server: &mut mockito::ServerGuard,
    json_rpc_version: &str,
    method: &str,
    params: impl Serialize,
    expected_response: &RpcResponse,
) -> mockito::Mock {
    let request_body = json!({
        "jsonrpc": json_rpc_version,
        "id": 0,
        "method": method,
        "params": json!(params),
    });
    server
        .mock("POST", "/")
        .match_header("Content-Type", "application/json")
        .match_body(mockito::Matcher::Json(request_body))
        .with_status(201)
        .with_body(serde_json::to_string(expected_response).unwrap())
        .create()
}

#[tokio::test]
async fn test_get_storage_at() {
    let mut server = run_rpc_server().await;
    let config = RpcStateReaderConfig { url: server.url(), ..Default::default() };

    let expected_result = felt!("0x999");

    let mock = mock_rpc_interaction(
        &mut server,
        &config.json_rpc_version,
        "starknet_getStorageAt",
        GetStorageAtParams {
            block_id: BlockId::Latest,
            contract_address: contract_address!("0x1"),
            key: starknet_api::state::StorageKey::from(0u32),
        },
        &RpcResponse::Success(RpcSuccessResponse {
            result: serde_json::to_value(expected_result).unwrap(),
            ..Default::default()
        }),
    );

    let client = rpc_state_reader_from_latest(&config);
    let result = tokio::task::spawn_blocking(move || {
        client.get_storage_at(contract_address!("0x1"), starknet_api::state::StorageKey::from(0u32))
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result, expected_result);
    mock.assert_async().await;
}

#[tokio::test]
async fn test_get_nonce_at() {
    let mut server = run_rpc_server().await;
    let config = RpcStateReaderConfig { url: server.url(), ..Default::default() };

    let expected_result = nonce!(0x999);

    let mock = mock_rpc_interaction(
        &mut server,
        &config.json_rpc_version,
        "starknet_getNonce",
        GetNonceParams { block_id: BlockId::Latest, contract_address: contract_address!("0x1") },
        &RpcResponse::Success(RpcSuccessResponse {
            result: serde_json::to_value(expected_result).unwrap(),
            ..Default::default()
        }),
    );

    let client = rpc_state_reader_from_latest(&config);
    let result = tokio::task::spawn_blocking(move || client.get_nonce_at(contract_address!("0x1")))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result, expected_result);
    mock.assert_async().await;
}

#[tokio::test]
async fn test_get_class_hash_at() {
    let mut server = run_rpc_server().await;
    let config = RpcStateReaderConfig { url: server.url(), ..Default::default() };

    let expected_result = class_hash!("0x999");

    let mock = mock_rpc_interaction(
        &mut server,
        &config.json_rpc_version,
        "starknet_getClassHashAt",
        GetClassHashAtParams {
            block_id: BlockId::Latest,
            contract_address: contract_address!("0x1"),
        },
        &RpcResponse::Success(RpcSuccessResponse {
            result: serde_json::to_value(expected_result).unwrap(),
            ..Default::default()
        }),
    );

    let client = rpc_state_reader_from_latest(&config);
    let result =
        tokio::task::spawn_blocking(move || client.get_class_hash_at(contract_address!("0x1")))
            .await
            .unwrap()
            .unwrap();
    assert_eq!(result, expected_result);
    mock.assert_async().await;
}
