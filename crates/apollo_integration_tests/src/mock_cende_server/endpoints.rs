use std::sync::Arc;

use apollo_starknet_client::reader::objects::block::BlockPostV0_13_1;
use apollo_starknet_client::reader::{Block, StateUpdate};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockNumber, NonzeroGasPrice};
use starknet_api::core::ClassHash;
use starknet_api::state::SierraContractClass;
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
async fn process_blob(storage: &MockCendeStorage, _blob: serde_json::Value) -> Result<(), String> {
    // TODO(noamsp): Implement this function.

    // Store both block and state update
    storage
        .add_block_data(
            BlockNumber(0),
            Block::PostV0_13_1(BlockPostV0_13_1 { ..Default::default() }),
            StateUpdate::default(),
            Vec::new(),
            Vec::new(),
            IndexMap::new(),
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
