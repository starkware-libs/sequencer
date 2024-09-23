use std::fs::File;

use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::blockifier::transaction_executor::TransactionExecutor;
use blockifier::context::{BlockContext, ChainInfo, FeeTokenAddresses};
use blockifier::execution::contract_class::ContractClass;
use blockifier::state::cached_state::CachedState;
use blockifier::state::state_api::StateReader;
use blockifier::transaction::account_transaction::AccountTransaction;
use blockifier::transaction::transaction_execution::Transaction;
use blockifier::transaction::transactions::InvokeTransaction;
use blockifier::versioned_constants::VersionedConstants;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use papyrus_rpc::CompiledContractClass;
use serde::Serialize;
use serde_json::json;
use starknet_api::block::{BlockNumber, GasPrice};
use starknet_api::core::{ChainId, ClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::{class_hash, contract_address, felt, patricia_key};
use starknet_types_core::felt::Felt;

use crate::config::RpcStateReaderConfig;
use crate::rpc_objects::{
    BlockHeader,
    BlockId,
    GetBlockWithTxHashesParams,
    GetClassHashAtParams,
    GetCompiledContractClassParams,
    GetNonceParams,
    GetStorageAtParams,
    ResourcePrice,
    RpcResponse,
    RpcSuccessResponse,
};
use crate::rpc_state_reader::RpcStateReader;
use crate::state_reader::MempoolStateReader;

pub const STRK_ADDRESS: &str = "0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d";
pub const ETH_ADDRESS: &str = "0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7";

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

    let expected_result =
        CasmContractClass { compiler_version: "0.0.0".to_string(), ..Default::default() };

    let mock = mock_rpc_interaction(
        &mut server,
        &config.json_rpc_version,
        "starknet_getCompiledContractClass",
        GetCompiledContractClassParams {
            block_id: BlockId::Latest,
            class_hash: class_hash!("0x1"),
        },
        &RpcResponse::Success(RpcSuccessResponse {
            result: serde_json::to_value(CompiledContractClass::V1(expected_result.clone()))
                .unwrap(),
            ..Default::default()
        }),
    );

    let client = RpcStateReader::from_latest(&config);
    let result =
        tokio::task::spawn_blocking(move || client.get_compiled_contract_class(class_hash!("0x1")))
            .await
            .unwrap()
            .unwrap();
    assert_eq!(result, ContractClass::V1(expected_result.try_into().unwrap()));
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

#[test]
fn rpc_state_reader() {
    let state_reader = RpcStateReader {
        config: RpcStateReaderConfig {
            url: "https://free-rpc.nethermind.io/mainnet-juno/".to_string(),
            json_rpc_version: "2.0".to_string(),
        },
        block_id: BlockId::Number(BlockNumber(700000)),
    };
    println!("{:?} ", state_reader.get_block_info());
}

#[test]
fn rpc_state_reader_storage() {
    let state_reader = RpcStateReader {
        config: RpcStateReaderConfig {
            url: "https://free-rpc.nethermind.io/mainnet-juno/".to_string(),
            json_rpc_version: "2.0".to_string(),
        },
        block_id: BlockId::Number(BlockNumber(700000)),
    };
    println!(
        "{:?} ",
        state_reader
            .get_storage_at(contract_address!("0x1"), starknet_api::state::StorageKey::from(0u32))
    );
}

#[test]
fn rerun_block() {
    let mut state_reader = RpcStateReader {
        config: RpcStateReaderConfig {
            url: "https://free-rpc.nethermind.io/mainnet-juno/".to_string(),
            json_rpc_version: "2.0".to_string(),
        },
        block_id: BlockId::Number(BlockNumber(700000)),
    };

    // let raw_txs_hash = File::open("./src/txs_hash_rpc.json").unwrap();
    // let txs_hash: Vec<String> = serde_json::from_reader(raw_txs_hash).unwrap();
    // state_reader.get_txs_by_hash(&txs_hash[0]);

    let raw_txs = File::open("./src/txs_feeder.json").unwrap();
    let mut tx_vec = serde_json::from_reader::<_, Vec<InvokeTransaction>>(raw_txs).unwrap();
    tx_vec.sort_by_key(|tx| tx.nonce());

    let tx_vec = tx_vec
        .iter()
        .map(|tx| Transaction::AccountTransaction(AccountTransaction::Invoke(tx.clone())))
        .collect::<Vec<_>>();

    let txs: &[Transaction] = tx_vec.as_slice();

    let block_info = state_reader.get_block_info().unwrap();
    let chain_id = ChainId::Mainnet;

    state_reader.block_id = BlockId::Number(BlockNumber(699999));

    let mut transaction_executor = TransactionExecutor::<RpcStateReader>::new(
        CachedState::new(state_reader),
        BlockContext::new(
            block_info,
            ChainInfo {
                chain_id,
                fee_token_addresses: FeeTokenAddresses {
                    strk_fee_token_address: ContractAddress::try_from(Felt::from_hex_unchecked(
                        STRK_ADDRESS,
                    ))
                    .unwrap(),
                    eth_fee_token_address: ContractAddress::try_from(Felt::from_hex_unchecked(
                        ETH_ADDRESS,
                    ))
                    .unwrap(),
                },
            },
            VersionedConstants::get(blockifier::versioned_constants::StarknetVersion::Latest)
                .clone(),
            blockifier::bouncer::BouncerConfig::max(),
        ),
        TransactionExecutorConfig::default(),
    );

    let tx_result = transaction_executor.execute_txs(txs);
    println!("{:?}", tx_result);
    let state_diff = transaction_executor.finalize().unwrap();
    println!("{:?}", state_diff);
}
