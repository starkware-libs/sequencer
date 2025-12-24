use std::sync::Arc;

use apollo_starknet_client::reader::objects::block::{BlockPostV0_13_1, BlockStatus};
use apollo_starknet_client::reader::{
    Block,
    DeclaredClassHashEntry,
    DeployedContract,
    StateDiff,
    StateUpdate,
    StorageEntry,
};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use starknet_api::block::{
    BlockHash,
    BlockNumber,
    BlockTimestamp,
    GasPrice,
    GasPricePerToken,
    NonzeroGasPrice,
    StarknetVersion,
};
use starknet_api::core::{
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    EventCommitment,
    GlobalRoot,
    Nonce,
    ReceiptCommitment,
    SequencerContractAddress,
    StateDiffCommitment,
    TransactionCommitment,
};
use starknet_api::data_availability::{DataAvailabilityMode, L1DataAvailabilityMode};
use starknet_api::execution_resources::GasAmount;
use starknet_api::hash::StarkHash;
use starknet_api::state::{SierraContractClass, StorageKey};
use starknet_types_core::felt::Felt;

use crate::mock_cende_server::storage::MockCendeStorage;

/// Query parameters for get_block endpoint
#[derive(Debug, Deserialize)]
#[allow(non_snake_case, dead_code)]
pub struct GetBlockParams {
    pub blockNumber: Option<String>,
    pub headerOnly: Option<bool>,
    pub withFeeMarketInfo: Option<bool>,
}

/// Query parameters for get_state_update endpoint
#[derive(Debug, Deserialize)]
#[allow(non_snake_case, dead_code)]
pub struct GetStateUpdateParams {
    pub blockNumber: Option<String>,
    pub includeBlock: Option<bool>,
}

/// Query parameters for get_class_by_hash endpoint
#[derive(Debug, Deserialize)]
#[allow(non_snake_case, dead_code)]
pub struct GetClassByHashParams {
    #[serde(rename = "classHash")]
    pub class_hash: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct CentralResourcePrice {
    price_in_wei: NonzeroGasPrice,
    price_in_fri: NonzeroGasPrice,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub(crate) struct CentralSierraContractClass {
    contract_class: SierraContractClass,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct CentralCasmContractClass {
    compiled_class: CasmContractClass,
}

/// Write blob endpoint handler
pub async fn write_blob(
    State(storage): State<Arc<MockCendeStorage>>,
    Json(blob): Json<serde_json::Value>,
) -> impl IntoResponse {
    match process_blob(&storage, blob).await {
        Ok(()) => (StatusCode::OK, "Blob written successfully"),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to process blob"),
    }
}

/// Process the blob and store block and state update data
async fn process_blob(storage: &MockCendeStorage, blob: serde_json::Value) -> Result<(), String> {
    // Extract block number
    let block_number = BlockNumber(
        blob.get("block_number").and_then(|v| v.as_u64()).expect("Missing or invalid block_number"),
    );

    let block_hash = BlockHash(StarkHash::from(block_number.0));
    let parent_hash = BlockHash(StarkHash::from(block_number.prev().unwrap_or(BlockNumber(0)).0));

    let central_state_diff =
        blob.get("state_diff").cloned().expect("Missing or invalid state_diff");

    let central_block_info =
        central_state_diff.get("block_info").cloned().expect("Missing or invalid block_info");
    let l1_gas_price =
        central_block_info.get("l1_gas_price").cloned().expect("Missing or invalid l1_gas_price");
    let l1_data_gas_price = central_block_info
        .get("l1_data_gas_price")
        .cloned()
        .expect("Missing or invalid l1_data_gas_price");
    let l2_gas_price =
        central_block_info.get("l2_gas_price").cloned().expect("Missing or invalid l2_gas_price");
    let l1_gas_price: CentralResourcePrice =
        serde_json::from_value(l1_gas_price).expect("Failed to deserialize l1_gas_price");
    let l1_data_gas_price: CentralResourcePrice =
        serde_json::from_value(l1_data_gas_price).expect("Failed to deserialize l1_data_gas_price");
    let l2_gas_price: CentralResourcePrice =
        serde_json::from_value(l2_gas_price).expect("Failed to deserialize l2_gas_price");

    let l1_gas_price = GasPricePerToken {
        price_in_wei: l1_gas_price.price_in_wei.into(),
        price_in_fri: l1_gas_price.price_in_fri.into(),
    };
    let l1_data_gas_price = GasPricePerToken {
        price_in_wei: l1_data_gas_price.price_in_wei.into(),
        price_in_fri: l1_data_gas_price.price_in_fri.into(),
    };
    let l2_gas_price = GasPricePerToken {
        price_in_wei: l2_gas_price.price_in_wei.into(),
        price_in_fri: l2_gas_price.price_in_fri.into(),
    };

    let fee_market_info =
        blob.get("fee_market_info").cloned().expect("Missing or invalid fee_market_info");
    let l2_gas_consumed = fee_market_info
        .get("l2_gas_consumed")
        .cloned()
        .expect("Missing or invalid l2_gas_consumed");
    let l2_gas_consumed: GasAmount =
        serde_json::from_value(l2_gas_consumed).expect("Failed to deserialize l2_gas_consumed");
    let next_l2_gas_price = fee_market_info
        .get("next_l2_gas_price")
        .cloned()
        .expect("Missing or invalid next_l2_gas_price");
    let next_l2_gas_price: GasPrice =
        serde_json::from_value(next_l2_gas_price).expect("Failed to deserialize next_l2_gas_price");

    let new_root = GlobalRoot(StarkHash::from(block_number.0));
    let old_root = GlobalRoot(StarkHash::from(block_number.prev().unwrap_or(BlockNumber(0)).0));

    let sequencer_address = central_block_info
        .get("sequencer_address")
        .cloned()
        .expect("Missing or invalid sequencer_address");
    let sequencer_address: ContractAddress =
        serde_json::from_value(sequencer_address).expect("Failed to deserialize sequencer_address");

    let block_timestamp =
        central_block_info.get("block_timestamp").cloned().expect("Missing or invalid timestamp");
    let block_timestamp: BlockTimestamp =
        serde_json::from_value(block_timestamp).expect("Failed to deserialize block_timestamp");

    let block = Block::PostV0_13_1(BlockPostV0_13_1 {
        block_hash,
        block_number,
        parent_block_hash: parent_hash,
        l1_data_gas_price,
        l2_gas_price,
        l1_gas_price,
        l2_gas_consumed,
        next_l2_gas_price,
        state_root: new_root,
        status: BlockStatus::AcceptedOnL2,
        sequencer_address: SequencerContractAddress(sequencer_address),
        timestamp: block_timestamp,
        starknet_version: StarknetVersion::V0_14_1,
        l1_da_mode: L1DataAvailabilityMode::Blob,
        transactions: vec![],
        transaction_receipts: vec![],
        transaction_commitment: TransactionCommitment::default(),
        event_commitment: EventCommitment::default(),
        state_diff_commitment: Some(StateDiffCommitment::default()),
        receipt_commitment: Some(ReceiptCommitment::default()),
        state_diff_length: Some(0),
    });

    let address_to_class_hash = central_state_diff
        .get("address_to_class_hash")
        .cloned()
        .expect("Missing or invalid address_to_class_hash");
    let address_to_class_hash: IndexMap<ContractAddress, ClassHash> =
        serde_json::from_value(address_to_class_hash)
            .expect("Failed to deserialize address_to_class_hash");
    let deployed_contracts: Vec<DeployedContract> = address_to_class_hash
        .into_iter()
        .map(|(address, class_hash)| DeployedContract { address, class_hash })
        .collect();

    let storage_updates = central_state_diff
        .get("storage_updates")
        .cloned()
        .expect("Missing or invalid storage_updates");
    let storage_updates: IndexMap<
        DataAvailabilityMode,
        IndexMap<ContractAddress, IndexMap<StorageKey, Felt>>,
    > = serde_json::from_value(storage_updates).expect("Failed to deserialize storage_updates");
    let storage_diffs: IndexMap<ContractAddress, Vec<StorageEntry>> = storage_updates
        .into_iter()
        .flat_map(|(_, storage_map)| {
            storage_map.into_iter().map(|(address, entries)| {
                (
                    address,
                    entries.into_iter().map(|(key, value)| StorageEntry { key, value }).collect(),
                )
            })
        })
        .collect();

    let class_hash_to_compiled_class_hash = central_state_diff
        .get("class_hash_to_compiled_class_hash")
        .cloned()
        .expect("Missing or invalid class_hash_to_compiled_class_hash");
    let class_hash_to_compiled_class_hash: IndexMap<ClassHash, CompiledClassHash> =
        serde_json::from_value(class_hash_to_compiled_class_hash)
            .expect("Failed to deserialize class_hash_to_compiled_class_hash");

    let contract_classes =
        blob.get("contract_classes").cloned().expect("Missing or invalid contract_classes");
    let contract_classes: Vec<(ClassHash, CentralSierraContractClass)> =
        serde_json::from_value(contract_classes).expect("Failed to deserialize contract_classes");
    let declared_classes: Vec<DeclaredClassHashEntry> = contract_classes
        .iter()
        .map(|(class_hash, _)| DeclaredClassHashEntry {
            class_hash: *class_hash,
            compiled_class_hash: *class_hash_to_compiled_class_hash
                .get(class_hash)
                .expect("Missing or invalid compiled_class_hash"),
        })
        .collect();

    let nonces = central_state_diff.get("nonces").cloned().expect("Missing or invalid nonces");
    let nonces: IndexMap<DataAvailabilityMode, IndexMap<ContractAddress, Nonce>> =
        serde_json::from_value(nonces).expect("Failed to deserialize nonces");
    let nonces: IndexMap<ContractAddress, Nonce> =
        nonces.into_iter().flat_map(|(_, nonce_map)| nonce_map.into_iter()).collect();

    let state_diff = StateDiff {
        deployed_contracts,
        storage_diffs,
        declared_classes,
        nonces,
        old_declared_contracts: vec![],
        replaced_classes: vec![],
        migrated_compiled_classes: vec![],
    };

    let state_update = StateUpdate { block_hash, new_root, old_root, state_diff };

    let contract_classes: Vec<(ClassHash, SierraContractClass)> = contract_classes
        .into_iter()
        .map(|(class_hash, contract_class)| (class_hash, contract_class.contract_class.clone()))
        .collect();
    let compiled_classes =
        blob.get("compiled_classes").cloned().expect("Missing or invalid compiled_classes");
    let compiled_classes: Vec<(CompiledClassHash, CentralCasmContractClass)> =
        serde_json::from_value(compiled_classes).expect("Failed to deserialize compiled_classes");
    let compiled_class_hash_to_compiled_class: Vec<(CompiledClassHash, CasmContractClass)> =
        compiled_classes
            .into_iter()
            .map(|(compiled_class_hash, compiled_class)| {
                (compiled_class_hash, compiled_class.compiled_class)
            })
            .collect();

    // Store both block and state update
    storage
        .add_block_data(
            block_number,
            block,
            state_update,
            contract_classes,
            compiled_class_hash_to_compiled_class,
            class_hash_to_compiled_class_hash,
        )
        .await;

    Ok(())
}

/// Get block endpoint handler
pub async fn get_block(
    State(storage): State<Arc<MockCendeStorage>>,
    Query(params): Query<GetBlockParams>,
) -> impl IntoResponse {
    let block_number = match parse_block_number(&params.blockNumber, storage.as_ref()).await {
        Ok(Some(num)) => num,
        Ok(None) => return (StatusCode::NOT_FOUND, "Block not found").into_response(),
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid block number").into_response(),
    };

    match storage.get_block(block_number).await {
        Some(block_str) => (StatusCode::OK, block_str).into_response(),
        None => (StatusCode::NOT_FOUND, "Block not found").into_response(),
    }
}

/// Get state update endpoint handler
pub async fn get_state_update(
    State(storage): State<Arc<MockCendeStorage>>,
    Query(params): Query<GetStateUpdateParams>,
) -> impl IntoResponse {
    let block_number = match parse_block_number(&params.blockNumber, storage.as_ref()).await {
        Ok(Some(num)) => num,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, "State update not found").into_response();
        }
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid block number").into_response(),
    };

    match storage.get_state_update(block_number).await {
        Some(state_update_str) => (StatusCode::OK, state_update_str).into_response(),
        None => (StatusCode::NOT_FOUND, "State update not found").into_response(),
    }
}

/// Get signature endpoint handler
pub async fn get_signature(
    State(storage): State<Arc<MockCendeStorage>>,
    Query(params): Query<GetBlockParams>,
) -> impl IntoResponse {
    let block_number = match parse_block_number(&params.blockNumber, storage.as_ref()).await {
        Ok(Some(num)) => num,
        Ok(None) => return (StatusCode::NOT_FOUND, "Block not found").into_response(),
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid block number").into_response(),
    };

    // Check if block exists
    if storage.get_block(block_number).await.is_none() {
        return (StatusCode::NOT_FOUND, "Block not found").into_response();
    }

    // Return dummy signature in V0_13_2 format
    let signature_response = serde_json::json!({
        "block_hash": format!("0x{:064x}", block_number.0),
        "signature": ["0x1", "0x2"]
    });

    (StatusCode::OK, Json(signature_response)).into_response()
}

/// is_alive health check endpoint
pub async fn is_alive() -> impl IntoResponse {
    (StatusCode::OK, "FeederGateway is alive!").into_response()
}

/// Get public key endpoint
pub async fn get_public_key() -> impl IntoResponse {
    // Return quoted hex string (32 bytes = 64 hex chars) - must be valid JSON string
    (StatusCode::OK, "\"0x0000000000000000000000000000000000000000000000000000000000000000\"")
        .into_response()
}

/// Get class by hash endpoint
pub async fn get_class_by_hash(
    State(storage): State<Arc<MockCendeStorage>>,
    Query(params): Query<GetClassByHashParams>,
) -> impl IntoResponse {
    let class_hash = match Felt::from_hex(&params.class_hash) {
        Ok(felt) => ClassHash(felt),
        Err(_) => return (StatusCode::BAD_REQUEST, "Failed to parse class_hash").into_response(),
    };
    match storage.get_contract_class(class_hash).await {
        Some(response) => (StatusCode::OK, response).into_response(),
        None => (StatusCode::NOT_FOUND, "Class not found").into_response(),
    }
}

/// Get compiled class by hash endpoint
pub async fn get_compiled_class_by_class_hash(
    State(storage): State<Arc<MockCendeStorage>>,
    Query(params): Query<GetClassByHashParams>,
) -> impl IntoResponse {
    let class_hash = match Felt::from_hex(&params.class_hash) {
        Ok(felt) => ClassHash(felt),
        Err(_) => return (StatusCode::BAD_REQUEST, "Failed to parse class_hash").into_response(),
    };
    match storage.get_compiled_class(class_hash).await {
        Some(response) => (StatusCode::OK, response).into_response(),
        None => (StatusCode::NOT_FOUND, "Compiled class not found").into_response(),
    }
}

/// Parse block number from string parameter
async fn parse_block_number(
    block_number_str: &Option<String>,
    storage: &MockCendeStorage,
) -> Result<Option<BlockNumber>, String> {
    match block_number_str {
        Some(s) => match s.as_str() {
            "latest" => Ok(storage.get_latest_block_number().await),
            "pending" => Ok(storage.get_latest_block_number().await),
            num_str => {
                let num = num_str.parse::<u64>().map_err(|_| "Invalid block number format")?;
                Ok(Some(BlockNumber(num)))
            }
        },
        None => Ok(storage.get_latest_block_number().await),
    }
}
