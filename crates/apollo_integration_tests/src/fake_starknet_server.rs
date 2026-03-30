//! Fake HTTP server for integration testing.
//!
//! [`FakeStarknetServer`] binds on a random port and serves two sets of endpoints:
//!
//! - **Feeder gateway** (`/feeder_gateway/*`): mirrors the Starknet feeder gateway protocol, so a
//!   `StarknetFeederGatewayClient` pointed at `server.url` works against it.
//! - **Cende recorder** (`/cende_recorder/*`): mirrors the Cende recorder protocol, so a
//!   `CendeAmbassador` pointed at `server.url` works against it.
//!
//! `state.blocks` is the Cende-populated store: blocks flow in via
//! `POST /cende_recorder/write_blob` and out via `get_block` / `get_latest_block`.
//! `get_latest_received_block` is derived from `blocks.keys().max()`.
//!
//! `state.state_updates` and `state.block_signatures` are separate maps populated by test code
//! and served by `get_state_update` and `get_signature` respectively. These endpoints exist for
//! feeder-gateway compatibility (`StarknetFeederGatewayClient` deserializes them into `StateUpdate`
//! and `BlockSignatureData`, which have different schemas from the Cende blob), so callers must
//! supply the correctly-shaped JSON directly.

#[cfg(test)]
#[path = "fake_starknet_server_test.rs"]
mod fake_starknet_server_test;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::{Json, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;
use serde_json::Value;
use tokio::net::TcpListener;
use url::Url;

// Error JSON bodies matching the Starknet feeder gateway error wire format.
pub const BLOCK_NOT_FOUND_JSON: &str =
    r#"{"code":"StarknetErrorCode.BLOCK_NOT_FOUND","message":"Block not found"}"#;
const UNDECLARED_CLASS_JSON: &str =
    r#"{"code":"StarknetErrorCode.UNDECLARED_CLASS","message":"Undeclared class"}"#;

/// Mutable data shared between server handlers and the [`FakeStarknetServer`] owner.
pub struct FakeServerState {
    /// Block store populated by `POST /cende_recorder/write_blob`.
    /// Drives `get_block` and `get_latest_block` feeder endpoints.
    pub blocks: HashMap<u64, Value>,

    /// Per-block `StateUpdate`-shaped JSON for `GET /feeder_gateway/get_state_update`.
    /// Must be populated by test code with the correct schema; not derived from `blocks`.
    pub state_updates: HashMap<u64, Value>,

    /// Per-block `BlockSignatureData`-shaped JSON for `GET /feeder_gateway/get_signature`.
    /// Must be populated by test code with the correct schema; not derived from `blocks`.
    pub block_signatures: HashMap<u64, Value>,

    /// Per-class-hash JSON responses for `get_class_by_hash`.
    pub classes_json: HashMap<String, String>,
    /// Per-class-hash JSON responses for `get_compiled_class_by_class_hash`.
    pub compiled_classes_json: HashMap<String, String>,
    /// Response for `get_state_update?blockNumber=pending`.
    pub pending_data_json: Option<String>,
    /// Response for `get_public_key`.
    pub sequencer_pub_key_json: Option<String>,

    /// When `false`, `POST /cende_recorder/write_blob` returns 500 without storing the blob.
    pub write_blob_should_succeed: bool,
}

impl FakeServerState {
    fn new() -> Self {
        Self {
            blocks: HashMap::new(),
            state_updates: HashMap::new(),
            block_signatures: HashMap::new(),
            classes_json: HashMap::new(),
            compiled_classes_json: HashMap::new(),
            pending_data_json: None,
            sequencer_pub_key_json: None,
            write_blob_should_succeed: true,
        }
    }

    /// The highest block number currently in the store, or `None` if the store is empty.
    pub fn latest_block_number(&self) -> Option<u64> {
        self.blocks.keys().copied().max()
    }
}

type SharedState = Arc<Mutex<FakeServerState>>;

/// A combined fake HTTP server with feeder-gateway and Cende-recorder endpoints.
///
/// Binds on a random loopback port on construction. Point both
/// `StarknetFeederGatewayClient` and `CendeAmbassador` at `server.url`.
pub struct FakeStarknetServer {
    /// Base URL of the server (e.g. `http://127.0.0.1:54321`).
    pub url: Url,
    /// Shared state: write here to configure responses, read here to inspect received blobs.
    pub state: SharedState,
    server_handle: tokio::task::JoinHandle<()>,
}

impl FakeStarknetServer {
    /// Spawns the server on an OS-assigned port. Must be called from within a tokio runtime.
    pub async fn new() -> Self {
        let state: SharedState = Arc::new(Mutex::new(FakeServerState::new()));

        let listener =
            TcpListener::bind("127.0.0.1:0").await.expect("Failed to bind fake server port");
        let addr = listener.local_addr().expect("Failed to read fake server local address");
        let url = Url::parse(&format!("http://{addr}")).expect("Failed to parse fake server URL");

        let app = build_router(state.clone());
        let server_handle = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("Fake server crashed");
        });

        Self { url, state, server_handle }
    }
}

impl Drop for FakeStarknetServer {
    fn drop(&mut self) {
        self.server_handle.abort();
    }
}

fn build_router(state: SharedState) -> Router {
    Router::new()
        // Feeder gateway
        .route("/feeder_gateway/get_block", get(handle_get_block))
        .route("/feeder_gateway/get_class_by_hash", get(handle_get_class_by_hash))
        .route(
            "/feeder_gateway/get_compiled_class_by_class_hash",
            get(handle_get_compiled_class_by_hash),
        )
        .route("/feeder_gateway/get_state_update", get(handle_get_state_update))
        .route("/feeder_gateway/is_alive", get(handle_feeder_is_alive))
        .route("/feeder_gateway/get_signature", get(handle_get_block_signature))
        .route("/feeder_gateway/get_public_key", get(handle_get_public_key))
        // Cende recorder
        .route(
            "/cende_recorder/get_latest_received_block",
            get(handle_get_latest_received_block),
        )
        .route("/cende_recorder/write_blob", post(handle_write_blob))
        .with_state(state)
}

// Feeder gateway handlers

#[derive(Deserialize)]
struct GetBlockParams {
    #[serde(rename = "blockNumber")]
    block_number: String,
    #[serde(rename = "headerOnly")]
    header_only: Option<String>,
}

async fn handle_get_block(
    State(state): State<SharedState>,
    Query(params): Query<GetBlockParams>,
) -> impl IntoResponse {
    let state = state.lock().expect("Fake server state poisoned");
    let block = match params.block_number.as_str() {
        "latest" => state.latest_block_number().and_then(|n| state.blocks.get(&n)),
        other => other.parse().ok().and_then(|n| state.blocks.get(&n)),
    };
    let json = block.map(|b| {
        if params.header_only.is_some() {
            // Return only the fields needed for BlockHashAndNumber.
            serde_json::json!({
                "block_number": b.get("block_number"),
                "block_hash": b.get("block_hash"),
            })
            .to_string()
        } else {
            b.to_string()
        }
    });
    json_or_block_not_found(json)
}

#[derive(Deserialize)]
struct ClassHashParams {
    #[serde(rename = "classHash")]
    class_hash: String,
}

async fn handle_get_class_by_hash(
    State(state): State<SharedState>,
    Query(params): Query<ClassHashParams>,
) -> impl IntoResponse {
    let state = state.lock().expect("Fake server state poisoned");
    match state.classes_json.get(&params.class_hash).cloned() {
        Some(json) => (StatusCode::OK, json),
        None => (StatusCode::BAD_REQUEST, UNDECLARED_CLASS_JSON.to_string()),
    }
}

async fn handle_get_compiled_class_by_hash(
    State(state): State<SharedState>,
    Query(params): Query<ClassHashParams>,
) -> impl IntoResponse {
    let state = state.lock().expect("Fake server state poisoned");
    match state.compiled_classes_json.get(&params.class_hash).cloned() {
        Some(json) => (StatusCode::OK, json),
        None => (StatusCode::BAD_REQUEST, UNDECLARED_CLASS_JSON.to_string()),
    }
}

#[derive(Deserialize)]
struct GetStateUpdateParams {
    #[serde(rename = "blockNumber")]
    block_number: String,
}

async fn handle_get_state_update(
    State(state): State<SharedState>,
    Query(params): Query<GetStateUpdateParams>,
) -> impl IntoResponse {
    let state = state.lock().expect("Fake server state poisoned");
    let json = if params.block_number == "pending" {
        state.pending_data_json.clone()
    } else {
        if let Ok(block_number) = params.block_number.parse() {
            state.state_updates.get(&block_number).map(|v| v.to_string())
        } else {
            return (StatusCode::BAD_REQUEST, "Malformed block number".to_string());
        }
    };
    json_or_block_not_found(json)
}

async fn handle_feeder_is_alive() -> impl IntoResponse {
    (StatusCode::OK, "FeederGateway is alive!")
}

#[derive(Deserialize)]
struct BlockNumberParams {
    #[serde(rename = "blockNumber")]
    block_number: u64,
}

async fn handle_get_block_signature(
    State(state): State<SharedState>,
    Query(params): Query<BlockNumberParams>,
) -> impl IntoResponse {
    let state = state.lock().expect("Fake server state poisoned");
    json_or_block_not_found(state.block_signatures.get(&params.block_number).map(|v| v.to_string()))
}

async fn handle_get_public_key(State(state): State<SharedState>) -> impl IntoResponse {
    let state = state.lock().expect("Fake server state poisoned");
    match state.sequencer_pub_key_json.clone() {
        Some(json) => (StatusCode::OK, json),
        None => {
            (StatusCode::INTERNAL_SERVER_ERROR, "Sequencer public key not configured".to_string())
        }
    }
}

// Cende recorder handlers

async fn handle_get_latest_received_block(State(state): State<SharedState>) -> impl IntoResponse {
    let state = state.lock().expect("Fake server state poisoned");
    let body = serde_json::json!({ "block_number": state.latest_block_number() });
    (StatusCode::OK, body.to_string())
}

async fn handle_write_blob(
    State(state): State<SharedState>,
    Json(blob): Json<Value>,
) -> impl IntoResponse {
    let mut state = state.lock().expect("Fake server state poisoned");
    if !state.write_blob_should_succeed {
        return StatusCode::INTERNAL_SERVER_ERROR;
    }
    let Some(block_number) = blob.get("block_number").and_then(Value::as_u64) else {
        return StatusCode::BAD_REQUEST;
    };
    state.blocks.insert(block_number, blob);
    StatusCode::OK
}

// Helpers

fn json_or_block_not_found(json: Option<String>) -> (StatusCode, String) {
    match json {
        Some(json) => (StatusCode::OK, json),
        None => (StatusCode::BAD_REQUEST, BLOCK_NOT_FOUND_JSON.to_string()),
    }
}
