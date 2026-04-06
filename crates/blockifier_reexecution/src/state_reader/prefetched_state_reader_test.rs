use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use blockifier::state::cached_state::StateMaps;
use serde_json::json;
use starknet_api::core::ChainId;
use starknet_api::transaction::TransactionHash;
use starknet_api::{class_hash, contract_address, felt, invoke_tx_args, nonce, storage_key};

use super::prefetched_state_reader::simulate_and_get_initial_reads;
use crate::state_reader::rpc_objects::{BlockId, RpcResponse, RpcSuccessResponse};
use crate::state_reader::rpc_state_reader::{RetryConfig, RpcStateReader};
use crate::utils::get_chain_info;

/// Tests the full `simulate_and_get_initial_reads` flow: HTTP request, JSON-RPC response
/// extraction, and deserialization into `StateMaps`.
/// The `initial_reads` fixture is an example per the Starknet spec v0.12
/// `starknet_simulateTransactions` response format.
#[tokio::test]
async fn test_simulate_and_get_initial_reads() {
    let mut server = mockito::Server::new_async().await;

    // Example initial_reads per Starknet spec v0.12.
    let initial_reads = json!({
        "storage": [
            {"contract_address": "0xabc", "key": "0x10", "value": "0x42"},
            {"contract_address": "0xabc", "key": "0x20", "value": "0x0"}
        ],
        "nonces": [{"contract_address": "0xabc", "nonce": "0x5"}],
        "class_hashes": [{"contract_address": "0xabc", "class_hash": "0xdef"}],
        "declared_contracts": [{"class_hash": "0xdef", "is_declared": true}]
    });

    let mock = server
        .mock("POST", "/")
        .match_header("Content-Type", "application/json")
        .with_status(201)
        .with_body(
            serde_json::to_string(&RpcResponse::Success(RpcSuccessResponse {
                result: json!({ "initial_reads": initial_reads }),
                ..Default::default()
            }))
            .unwrap(),
        )
        .create();

    let config = crate::state_reader::config::RpcStateReaderConfig {
        url: server.url(),
        ..Default::default()
    };
    let rpc_state_reader = RpcStateReader {
        config,
        block_id: BlockId::Latest,
        retry_config: RetryConfig::default(),
        chain_info: get_chain_info(&ChainId::Mainnet, None),
        contract_class_mapping_dumper: Arc::new(Mutex::new(None)),
    };

    let invoke_args = invoke_tx_args!();
    let tx = starknet_api::test_utils::invoke::invoke_tx(invoke_args);
    let tx_hash = TransactionHash::default();

    let state_maps = tokio::task::spawn_blocking(move || {
        simulate_and_get_initial_reads(&rpc_state_reader, BlockId::Latest, &[(tx, tx_hash)], false)
    })
    .await
    .unwrap()
    .unwrap();

    let expected = StateMaps {
        storage: HashMap::from([
            ((contract_address!("0xabc"), storage_key!("0x10")), felt!("0x42")),
            ((contract_address!("0xabc"), storage_key!("0x20")), felt!("0x0")),
        ]),
        nonces: HashMap::from([(contract_address!("0xabc"), nonce!(0x5_u64))]),
        class_hashes: HashMap::from([(contract_address!("0xabc"), class_hash!("0xdef"))]),
        declared_contracts: HashMap::from([(class_hash!("0xdef"), true)]),
        compiled_class_hashes: HashMap::default(),
    };

    assert_eq!(state_maps, expected);
    mock.assert_async().await;
}
