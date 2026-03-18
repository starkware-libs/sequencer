use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use apollo_batcher::pre_confirmed_cende_client::RECORDER_WRITE_PRE_CONFIRMED_BLOCK_PATH;
use apollo_consensus_orchestrator::cende::RECORDER_WRITE_BLOB_PATH;
use apollo_starknet_client::reader::objects::block::{BlockPostV0_13_1, BlockStatus};
use apollo_starknet_client::reader::objects::state::{
    DeclaredClassHashEntry,
    DeployedContract,
    StateDiff,
    StateUpdate,
    StorageEntry,
};
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{serve, Json, Router};
use indexmap::IndexMap;
use starknet_api::block::{
    BlockHash,
    BlockHashAndNumber,
    BlockNumber,
    GasPrice,
    GasPricePerToken,
};
use starknet_api::core::{
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    GlobalRoot,
    Nonce,
};
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_types_core::felt::Felt;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use url::Url;

pub type BlockStore = Arc<Mutex<BTreeMap<u64, serde_json::Value>>>;

pub struct MockCentralSyncServer {
    pub url: Url,
    pub store: BlockStore,
    pub _handle: JoinHandle<()>,
}

pub fn spawn_mock_central_sync_server(port: u16) -> MockCentralSyncServer {
    let block_store: BlockStore = Arc::new(Mutex::new(BTreeMap::new()));
    let socket_address = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port);
    let url = Url::parse(&format!("http://{socket_address}")).unwrap();

    let router = Router::new()
        .route(RECORDER_WRITE_BLOB_PATH, post(write_blob_handler))
        .route(
            RECORDER_WRITE_PRE_CONFIRMED_BLOCK_PATH,
            post(|| async { StatusCode::OK }),
        )
        .route("/feeder_gateway/get_block", get(get_block_handler))
        .route("/feeder_gateway/get_state_update", get(get_state_update_handler))
        .route("/feeder_gateway/get_class_by_hash", get(get_class_handler))
        .route(
            "/feeder_gateway/get_compiled_class_by_class_hash",
            get(get_compiled_class_handler),
        )
        .route("/feeder_gateway/get_latest_block", get(get_latest_block_handler))
        .with_state(block_store.clone());

    let handle = tokio::spawn(async move {
        let listener = TcpListener::bind(socket_address).await.unwrap();
        serve(listener, router).await.unwrap();
    });

    MockCentralSyncServer { url, store: block_store, _handle: handle }
}

async fn write_blob_handler(
    State(block_store): State<BlockStore>,
    body: Bytes,
) -> StatusCode {
    let value: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => return StatusCode::BAD_REQUEST,
    };

    let block_number = match value.get("block_number").and_then(|v| v.as_u64()) {
        Some(number) => number,
        None => return StatusCode::BAD_REQUEST,
    };

    block_store.lock().unwrap().insert(block_number, value);
    StatusCode::OK
}

#[derive(serde::Deserialize)]
struct BlockNumberQuery {
    #[serde(rename = "blockNumber")]
    block_number: u64,
}

#[derive(serde::Deserialize)]
struct ClassHashQuery {
    #[serde(rename = "classHash")]
    class_hash: String,
}

async fn get_latest_block_handler(
    State(store): State<BlockStore>,
) -> Result<Json<BlockHashAndNumber>, StatusCode> {
    let store = store.lock().unwrap();
    let Some((&block_number, _)) = store.iter().next_back() else {
        return Err(StatusCode::NOT_FOUND);
    };
    Ok(Json(BlockHashAndNumber {
        number: BlockNumber(block_number),
        hash: BlockHash(Felt::from(block_number)),
    }))
}

async fn get_block_handler(
    State(store): State<BlockStore>,
    Query(params): Query<BlockNumberQuery>,
) -> Result<Json<BlockPostV0_13_1>, StatusCode> {
    let store = store.lock().unwrap();
    let blob = store.get(&params.block_number).ok_or(StatusCode::NOT_FOUND)?;
    let block = construct_block(params.block_number, blob);
    Ok(Json(block))
}

async fn get_state_update_handler(
    State(store): State<BlockStore>,
    Query(params): Query<BlockNumberQuery>,
) -> Result<Json<StateUpdate>, StatusCode> {
    let store = store.lock().unwrap();
    let blob = store.get(&params.block_number).ok_or(StatusCode::NOT_FOUND)?;
    let state_update = construct_state_update(params.block_number, blob);
    Ok(Json(state_update))
}

async fn get_class_handler(
    State(store): State<BlockStore>,
    Query(params): Query<ClassHashQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = store.lock().unwrap();
    let class_hash_lower = params.class_hash.to_lowercase();
    for blob in store.values() {
        if let Some(entries) = blob["contract_classes"].as_array() {
            for entry in entries {
                let entry_hash = entry[0].as_str().unwrap_or("").to_lowercase();
                if entry_hash == class_hash_lower {
                    let class_value = if entry[1]["contract_class"].is_object() {
                        entry[1]["contract_class"].clone()
                    } else {
                        entry[1].clone()
                    };
                    return Ok(Json(class_value));
                }
            }
        }
    }
    Err(StatusCode::NOT_FOUND)
}

async fn get_compiled_class_handler(
    State(store): State<BlockStore>,
    Query(params): Query<ClassHashQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = store.lock().unwrap();
    let class_hash_lower = params.class_hash.to_lowercase();
    for blob in store.values() {
        if let Some(entries) = blob["compiled_classes"].as_array() {
            for entry in entries {
                let entry_hash = entry[0].as_str().unwrap_or("").to_lowercase();
                if entry_hash == class_hash_lower {
                    let class_value = if entry[1]["compiled_class"].is_object() {
                        entry[1]["compiled_class"].clone()
                    } else {
                        entry[1].clone()
                    };
                    return Ok(Json(class_value));
                }
            }
        }
    }
    Err(StatusCode::NOT_FOUND)
}

fn parse_gas_price(value: &serde_json::Value) -> GasPrice {
    serde_json::from_value::<GasPrice>(value.clone()).unwrap_or(GasPrice(1))
}

fn parse_gas_price_per_token(gas_price_json: &serde_json::Value) -> GasPricePerToken {
    GasPricePerToken {
        price_in_wei: parse_gas_price(&gas_price_json["price_in_wei"]),
        price_in_fri: parse_gas_price(&gas_price_json["price_in_fri"]),
    }
}

fn extract_transactions(
    blob: &serde_json::Value,
) -> Vec<apollo_starknet_client::reader::objects::transaction::Transaction> {
    let Some(txs) = blob["transactions"].as_array() else {
        return vec![];
    };
    txs.iter()
        .filter_map(|tx_written| serde_json::from_value(tx_written["tx"].clone()).ok())
        .collect()
}

fn construct_block(block_number: u64, blob: &serde_json::Value) -> BlockPostV0_13_1 {
    let block_info = &blob["state_diff"]["block_info"];

    let use_kzg_da = block_info["use_kzg_da"].as_bool().unwrap_or(false);
    let l1_da_mode =
        if use_kzg_da { L1DataAvailabilityMode::Blob } else { L1DataAvailabilityMode::Calldata };

    let timestamp = block_info["block_timestamp"]
        .as_u64()
        .unwrap_or(0);

    let starknet_version = block_info["starknet_version"]
        .as_str()
        .and_then(|s| starknet_api::block::StarknetVersion::try_from(s).ok())
        .unwrap_or_default();

    let sequencer_address = block_info["sequencer_address"]
        .as_str()
        .and_then(|s| serde_json::from_value(serde_json::Value::String(s.to_string())).ok())
        .unwrap_or_default();

    BlockPostV0_13_1 {
        block_hash: BlockHash(Felt::from(block_number)),
        block_number: BlockNumber(block_number),
        parent_block_hash: BlockHash(Felt::from(block_number.saturating_sub(1))),
        sequencer_address,
        state_root: GlobalRoot::default(),
        status: BlockStatus::AcceptedOnL2,
        timestamp: starknet_api::block::BlockTimestamp(timestamp),
        transactions: extract_transactions(blob),
        transaction_receipts: vec![],
        starknet_version,
        l1_da_mode,
        l1_gas_price: parse_gas_price_per_token(&block_info["l1_gas_price"]),
        l1_data_gas_price: parse_gas_price_per_token(&block_info["l1_data_gas_price"]),
        l2_gas_price: parse_gas_price_per_token(&block_info["l2_gas_price"]),
        transaction_commitment: Default::default(),
        event_commitment: Default::default(),
        state_diff_commitment: None,
        receipt_commitment: None,
        state_diff_length: None,
        l2_gas_consumed: Default::default(),
        next_l2_gas_price: GasPrice(1),
    }
}

fn construct_state_update(block_number: u64, blob: &serde_json::Value) -> StateUpdate {
    StateUpdate {
        block_hash: BlockHash(Felt::from(block_number)),
        new_root: GlobalRoot::default(),
        old_root: GlobalRoot::default(),
        state_diff: extract_state_diff(&blob["state_diff"]),
    }
}

fn extract_state_diff(state_diff_json: &serde_json::Value) -> StateDiff {
    let storage_diffs = extract_storage_diffs(state_diff_json);
    let nonces = extract_nonces(state_diff_json);
    let deployed_contracts = extract_deployed_contracts(state_diff_json);
    let declared_classes = extract_declared_classes(state_diff_json);

    StateDiff {
        storage_diffs,
        deployed_contracts,
        declared_classes,
        migrated_compiled_classes: vec![],
        old_declared_contracts: vec![],
        nonces,
        replaced_classes: vec![],
    }
}

fn extract_storage_diffs(
    state_diff_json: &serde_json::Value,
) -> IndexMap<ContractAddress, Vec<StorageEntry>> {
    let mut result: IndexMap<ContractAddress, Vec<StorageEntry>> = IndexMap::new();
    let Some(storage_updates) = state_diff_json["storage_updates"].as_object() else {
        return result;
    };
    for (_da_mode, contracts) in storage_updates {
        let Some(contracts_map) = contracts.as_object() else {
            continue;
        };
        for (addr_str, slots) in contracts_map {
            let Ok(address) =
                serde_json::from_value::<ContractAddress>(serde_json::Value::String(addr_str.clone()))
            else {
                continue;
            };
            let Some(slots_map) = slots.as_object() else {
                continue;
            };
            let entries: Vec<StorageEntry> = slots_map
                .iter()
                .filter_map(|(key_str, val)| {
                    let key = serde_json::from_value(serde_json::Value::String(key_str.clone()))
                        .ok()?;
                    let value = serde_json::from_value(val.clone()).ok()?;
                    Some(StorageEntry { key, value })
                })
                .collect();
            result.entry(address).or_default().extend(entries);
        }
    }
    result
}

fn extract_nonces(state_diff_json: &serde_json::Value) -> IndexMap<ContractAddress, Nonce> {
    let mut result: IndexMap<ContractAddress, Nonce> = IndexMap::new();
    let Some(nonces_map) = state_diff_json["nonces"].as_object() else {
        return result;
    };
    for (_da_mode, contracts) in nonces_map {
        let Some(contracts_map) = contracts.as_object() else {
            continue;
        };
        for (addr_str, nonce_val) in contracts_map {
            let Ok(address) =
                serde_json::from_value::<ContractAddress>(serde_json::Value::String(addr_str.clone()))
            else {
                continue;
            };
            let Ok(nonce) = serde_json::from_value::<Nonce>(nonce_val.clone()) else {
                continue;
            };
            result.insert(address, nonce);
        }
    }
    result
}

fn extract_deployed_contracts(state_diff_json: &serde_json::Value) -> Vec<DeployedContract> {
    let Some(map) = state_diff_json["address_to_class_hash"].as_object() else {
        return vec![];
    };
    map.iter()
        .filter_map(|(addr_str, class_hash_val)| {
            let address = serde_json::from_value::<ContractAddress>(serde_json::Value::String(
                addr_str.clone(),
            ))
            .ok()?;
            let class_hash = serde_json::from_value::<ClassHash>(class_hash_val.clone()).ok()?;
            Some(DeployedContract { address, class_hash })
        })
        .collect()
}

fn extract_declared_classes(state_diff_json: &serde_json::Value) -> Vec<DeclaredClassHashEntry> {
    let Some(map) = state_diff_json["class_hash_to_compiled_class_hash"].as_object() else {
        return vec![];
    };
    map.iter()
        .filter_map(|(class_hash_str, compiled_class_hash_val)| {
            let class_hash = serde_json::from_value::<ClassHash>(serde_json::Value::String(
                class_hash_str.clone(),
            ))
            .ok()?;
            let compiled_class_hash =
                serde_json::from_value::<CompiledClassHash>(compiled_class_hash_val.clone()).ok()?;
            Some(DeclaredClassHashEntry { class_hash, compiled_class_hash })
        })
        .collect()
}
