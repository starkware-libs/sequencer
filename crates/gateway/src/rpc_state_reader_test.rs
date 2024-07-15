use blockifier::execution::contract_class::ContractClass;
use blockifier::state::state_api::StateReader;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use papyrus_rpc::CompiledContractClass;
use serde::Serialize;
use serde_json::json;
use starknet_api::block::{BlockNumber, GasPrice};
use starknet_api::core::{ClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::{class_hash, contract_address, felt, patricia_key};

use crate::config::RpcStateReaderConfig;
use crate::rpc_objects::{
    BlockHeader, BlockId, GetBlockWithTxHashesParams, GetClassHashAtParams,
    GetCompiledContractClassParams, GetNonceParams, GetStorageAtParams, ResourcePrice, RpcResponse,
    RpcSuccessResponse,
};
use crate::rpc_state_reader::RpcStateReader;
use crate::state_reader::MempoolStateReader;

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
async fn test_get_block_info() {
    let mut server = run_rpc_server().await;
    let config = RpcStateReaderConfig { url: server.url(), ..Default::default() };

    let expected_result = BlockNumber(100);

    let mock = mock_rpc_interaction(
        &mut server,
        &config.json_rpc_version,
        "starknet_getBlockWithTxHashes",
        GetBlockWithTxHashesParams { block_id: BlockId::Latest },
        &RpcResponse::Success(RpcSuccessResponse {
            result: serde_json::to_value(BlockHeader {
                block_number: expected_result,
                // GasPrice must be non-zero.
                l1_gas_price: ResourcePrice {
                    price_in_wei: GasPrice(1),
                    price_in_fri: GasPrice(1),
                },
                l1_data_gas_price: ResourcePrice {
                    price_in_wei: GasPrice(1),
                    price_in_fri: GasPrice(1),
                },
                ..Default::default()
            })
            .unwrap(),
            ..Default::default()
        }),
    );

    let client = RpcStateReader::from_latest(&config);
    let result =
        tokio::task::spawn_blocking(move || client.get_block_info()).await.unwrap().unwrap();
    // TODO(yair): Add partial_eq for BlockInfo and assert_eq the whole BlockInfo.
    assert_eq!(result.block_number, expected_result);
    mock.assert_async().await;
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

    let client = RpcStateReader::from_latest(&config);
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

    let expected_result = Nonce(felt!("0x999"));

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

    let client = RpcStateReader::from_latest(&config);
    let result = tokio::task::spawn_blocking(move || client.get_nonce_at(contract_address!("0x1")))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result, expected_result);
    mock.assert_async().await;
}

#[tokio::test]
async fn test_get_compiled_contract_class() {
    let mut server = run_rpc_server().await;
    let config = RpcStateReaderConfig { url: server.url(), ..Default::default() };

    let expected_result = CasmContractClass::default();

    let mock = mock_rpc_interaction(
        &mut server,
        &config.json_rpc_version,
        "starknet_getCompiledContractClass",
        GetCompiledContractClassParams {
            block_id: BlockId::Latest,
            class_hash: class_hash!("0x1"),
        },
        &RpcResponse::Success(RpcSuccessResponse {
            result: serde_json::to_value(CompiledContractClass::V1(expected_result)).unwrap(),
            ..Default::default()
        }),
    );

    let client = RpcStateReader::from_latest(&config);
    let result =
        tokio::task::spawn_blocking(move || client.get_compiled_contract_class(class_hash!("0x1")))
            .await
            .unwrap()
            .unwrap();
    assert_eq!(result, ContractClass::V1(CasmContractClass::default().try_into().unwrap()));
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

    let client = RpcStateReader::from_latest(&config);
    let result =
        tokio::task::spawn_blocking(move || client.get_class_hash_at(contract_address!("0x1")))
            .await
            .unwrap()
            .unwrap();
    assert_eq!(result, expected_result);
    mock.assert_async().await;
}
