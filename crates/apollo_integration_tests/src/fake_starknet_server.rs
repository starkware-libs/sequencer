//! Fake HTTP server for integration testing.
//!
//! [`FakeStarknetServer`] binds on a random port and serves two sets of endpoints:
//!
//! - **Feeder gateway** (`/feeder_gateway/*`): mirrors the Starknet feeder gateway protocol, so a
//!   `StarknetFeederGatewayClient` pointed at `server.url` works against it.
//! - **Cende recorder** (`/cende_recorder/*`): mirrors the Cende recorder protocol, so a
//!   `CendeAmbassador` pointed at `server.url` works against it.
//!
//! Both endpoint sets share a single [`FakeBlock`] store keyed by block number. Each block
//! accumulates data from three independent sources:
//!
//! - **`block_hash`**: filled in when a later blob's `recent_block_hashes` references this block
//!   number. Until a hash arrives the block is invisible to feeder endpoints and
//!   `get_latest_received_block`.
//! - **`feeder_json`**: seeded by test code with correctly-shaped feeder JSON.
//! - **`state_update`**: either seeded by test code OR automatically derived from a blob's
//!   `state_diff` field (converted from `CentralStateDiff` to feeder `StateUpdate` format).
//!
//! Any source may arrive first; each write touches only its own field so no data is lost.
//!
//! Serving rules:
//! - `get_latest_received_block`: max block number whose `block_hash` is `Some`.
//! - `get_block`: requires both `block_hash` and `feeder_json`. `feeder_json` is derived
//!   automatically from the blob posted to `write_blob` (header fields only; transactions are
//!   omitted). The `block_hash` field inside `feeder_json` is patched in when a later blob's
//!   `recent_block_hashes` confirms it, so `get_block` becomes available at the same moment as
//!   `get_state_update`.
//! - `get_state_update`: requires both `block_hash` and `state_update`.
//! - `get_signature`: always returns [`BLOCK_SIGNATURE_JSON`] (signature is deprecated).

#[cfg(test)]
#[path = "fake_starknet_server_test.rs"]
mod fake_starknet_server_test;

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::extract::{Json, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;
use serde_json::Value;
use tokio::net::TcpListener;
use url::Url;

// Error JSON bodies matching the Starknet feeder gateway error wire format.
pub const BLOCK_NOT_FOUND_JSON: &str =
    r#"{"code":"StarknetErrorCode.BLOCK_NOT_FOUND","message":"Block not found"}"#;
pub const UNDECLARED_CLASS_JSON: &str =
    r#"{"code":"StarknetErrorCode.UNDECLARED_CLASS","message":"Undeclared class"}"#;
/// Constant returned by `get_signature`. Block signatures are deprecated.
pub const BLOCK_SIGNATURE_JSON: &str =
    r#"{"signature":["0x0","0x0"],"signature_input":{"block_hash":"0x0"}}"#;

/// Per-block data accumulating from three independent sources: blob posts and test seeding.
/// Fields are always written individually so earlier data is never overwritten by later writes.
#[derive(Default)]
pub struct FakeBlock {
    /// Confirmed block hash. Set when a later blob's `recent_block_hashes` references this block.
    /// `None` means the block's existence is recorded but its hash is not yet known.
    pub block_hash: Option<String>,
    /// Full feeder-format JSON for `GET /feeder_gateway/get_block`. Seeded by test code.
    pub feeder_json: Option<Value>,
    /// `StateUpdate`-shaped JSON for `GET /feeder_gateway/get_state_update`.
    /// Either seeded by test code OR derived from a blob's `state_diff` field.
    pub state_update: Option<Value>,
}

/// Mutable data shared between server handlers and the [`FakeStarknetServer`] owner.
pub struct FakeServerState {
    /// All known blocks, keyed by block number. A block entry is created the first time its number
    /// appears in a blob post or is seeded by test code. Its `block_hash` is set when a later
    /// blob's `recent_block_hashes` references it.
    pub blocks: HashMap<u64, FakeBlock>,

    /// Per-class-hash JSON responses for `get_class_by_hash`.
    pub classes_json: HashMap<String, Value>,
    /// Per-class-hash JSON responses for `get_compiled_class_by_class_hash`.
    pub compiled_classes_json: HashMap<String, Value>,
    /// Response for `get_state_update?blockNumber=pending`.
    pub pending_data_json: Option<Value>,
    /// Response for `get_public_key`.
    pub sequencer_pub_key_json: Option<Value>,

    /// When `false`, `POST /cende_recorder/write_blob` returns 500 without storing anything.
    pub write_blob_should_succeed: bool,

    /// Block numbers received via `POST /cende_recorder/write_pre_confirmed_block`.
    pub pre_confirmed_block_numbers: HashSet<u64>,
    /// When `false`, `POST /cende_recorder/write_pre_confirmed_block` returns 500.
    pub write_pre_confirmed_block_should_succeed: bool,
}

impl FakeServerState {
    fn new() -> Self {
        Self {
            blocks: HashMap::new(),
            classes_json: HashMap::new(),
            compiled_classes_json: HashMap::new(),
            pending_data_json: None,
            sequencer_pub_key_json: None,
            write_blob_should_succeed: true,
            pre_confirmed_block_numbers: HashSet::new(),
            write_pre_confirmed_block_should_succeed: true,
        }
    }

    /// The highest block number with a confirmed hash, or `None` if no block has a hash yet.
    pub fn latest_cende_block_number(&self) -> Option<u64> {
        self.blocks.iter().filter(|(_, b)| b.block_hash.is_some()).map(|(n, _)| *n).max()
    }

    /// Seeds a block with both a confirmed hash and full feeder JSON for test setup.
    /// Uses field-level writes so it merges correctly with any data already in the entry
    /// (e.g., `state_update` derived from a blob's `state_diff`).
    pub fn seed_block(&mut self, block_number: u64, block_hash: &str, feeder_json: Value) {
        let block = self.blocks.entry(block_number).or_default();
        block.block_hash = Some(block_hash.to_string());
        block.feeder_json = Some(feeder_json);
    }
}

type SharedState = Arc<Mutex<FakeServerState>>;

/// A combined fake HTTP server with feeder-gateway and Cende-recorder endpoints.
///
/// Point both `StarknetFeederGatewayClient` and `CendeAmbassador` at `server.url`.
pub struct FakeStarknetServer {
    /// Base URL of the server (e.g. `http://127.0.0.1:54321`).
    pub url: Url,
    /// Shared state: write here to configure responses, read here to inspect received blobs.
    pub state: SharedState,
    server_handle: tokio::task::JoinHandle<()>,
}

impl FakeStarknetServer {
    /// Spawns the server bound to `addr`. Pass port `0` to let the OS assign a free port.
    /// Must be called from within a tokio runtime.
    pub async fn new(addr: SocketAddr) -> Self {
        let state: SharedState = Arc::new(Mutex::new(FakeServerState::new()));

        let listener = TcpListener::bind(addr).await.expect("Failed to bind fake server port");
        let addr = listener.local_addr().expect("Failed to read fake server local address");
        let url = Url::parse(&format!("http://{addr}")).expect("Failed to parse fake server URL");

        let app = build_router(state.clone());
        let server_handle = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("Fake server crashed");
        });

        Self { url, state, server_handle }
    }

    /// Runs until the server task exits (or panics).
    pub async fn run_until_exit(&mut self) {
        (&mut self.server_handle).await.expect("The fake server has panicked!");
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
        .route(
            "/cende_recorder/write_pre_confirmed_block",
            post(handle_write_pre_confirmed_block),
        )
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
    // Only blocks with both a confirmed hash and feeder content are served.
    let block = match params.block_number.as_str() {
        "latest" => state
            .blocks
            .iter()
            .filter(|(_, b)| b.block_hash.is_some() && b.feeder_json.is_some())
            .max_by_key(|(n, _)| *n)
            .map(|(_, b)| b),
        other => other
            .parse::<u64>()
            .ok()
            .and_then(|n| state.blocks.get(&n))
            .filter(|b| b.block_hash.is_some() && b.feeder_json.is_some()),
    };
    let json = block.map(|b| {
        let feeder_json = b.feeder_json.as_ref().expect("filtered for feeder_json.is_some()");
        if params.header_only.as_deref() == Some("true") {
            serde_json::json!({
                "block_number": feeder_json.get("block_number"),
                "block_hash": feeder_json.get("block_hash"),
            })
            .to_string()
        } else {
            feeder_json.to_string()
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
    match state.classes_json.get(&params.class_hash) {
        Some(json) => json_response(StatusCode::OK, json.to_string()),
        None => json_response(StatusCode::BAD_REQUEST, UNDECLARED_CLASS_JSON.to_string()),
    }
}

async fn handle_get_compiled_class_by_hash(
    State(state): State<SharedState>,
    Query(params): Query<ClassHashParams>,
) -> impl IntoResponse {
    let state = state.lock().expect("Fake server state poisoned");
    match state.compiled_classes_json.get(&params.class_hash) {
        Some(json) => json_response(StatusCode::OK, json.to_string()),
        None => json_response(StatusCode::BAD_REQUEST, UNDECLARED_CLASS_JSON.to_string()),
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
        state.pending_data_json.as_ref().map(|v| v.to_string())
    } else if let Ok(block_number) = params.block_number.parse::<u64>() {
        state
            .blocks
            .get(&block_number)
            .filter(|b| b.block_hash.is_some())
            .and_then(|b| b.state_update.as_ref())
            .map(|v| v.to_string())
    } else {
        None
    };
    json_or_block_not_found(json)
}

async fn handle_feeder_is_alive() -> impl IntoResponse {
    (StatusCode::OK, "FeederGateway is alive!")
}

async fn handle_get_block_signature() -> impl IntoResponse {
    // Block signatures are deprecated; always return a fixed constant.
    json_response(StatusCode::OK, BLOCK_SIGNATURE_JSON.to_string())
}

async fn handle_get_public_key(State(state): State<SharedState>) -> impl IntoResponse {
    let state = state.lock().expect("Fake server state poisoned");
    match state.sequencer_pub_key_json.as_ref() {
        Some(json) => json_response(StatusCode::OK, json.to_string()),
        None => json_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "\"Sequencer public key not configured\"".to_string(),
        ),
    }
}

// Cende recorder handlers

async fn handle_get_latest_received_block(State(state): State<SharedState>) -> impl IntoResponse {
    let state = state.lock().expect("Fake server state poisoned");
    let body = serde_json::json!({ "block_number": state.latest_cende_block_number() });
    json_response(StatusCode::OK, body.to_string())
}

async fn handle_write_blob(State(state): State<SharedState>, Json(blob): Json<Value>) -> Response {
    let mut state = state.lock().expect("Fake server state poisoned");
    if !state.write_blob_should_succeed {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let Some(block_number) = blob.get("block_number").and_then(Value::as_u64) else {
        return json_response(
            StatusCode::BAD_REQUEST,
            r#"{"error":"blob missing block_number"}"#.to_string(),
        )
        .into_response();
    };
    // Record the blob's block as existing, without a confirmed hash yet.
    state.blocks.entry(block_number).or_default();
    // Derive state_update and feeder_json from the blob's CentralStateDiff if not already set.
    if let Some(central_state_diff) = blob.get("state_diff") {
        let block = state.blocks.entry(block_number).or_default();
        if block.state_update.is_none() {
            let mut state_update = central_state_diff_to_feeder_state_update(central_state_diff);
            // If the hash was already confirmed by an earlier blob, patch it in now.
            if let Some(hash) = &block.block_hash {
                state_update["block_hash"] = Value::String(hash.clone());
            }
            block.state_update = Some(state_update);
        }
        if block.feeder_json.is_none() {
            let mut feeder_json = blob_to_feeder_block_json(&blob, block_number);
            // If the hash was already confirmed by an earlier blob, patch it in now.
            if let Some(hash) = &block.block_hash {
                feeder_json["block_hash"] = Value::String(hash.clone());
            }
            block.feeder_json = Some(feeder_json);
        }
    }
    // Fill in confirmed hashes for older blocks referenced by this blob.
    if let Some(recent_hashes) = blob.get("recent_block_hashes").and_then(Value::as_array) {
        for entry in recent_hashes {
            let Some(n) = entry.get("block_number").and_then(Value::as_u64) else { continue };
            let Some(h) = entry.get("block_hash").and_then(Value::as_str) else { continue };
            let block = state.blocks.entry(n).or_default();
            block.block_hash = Some(h.to_string());
            // Patch the confirmed hash into any state_update already derived for this block.
            if let Some(state_update) = &mut block.state_update {
                state_update["block_hash"] = Value::String(h.to_string());
            }
            // Patch the confirmed hash into any feeder_json already derived for this block.
            if let Some(feeder_json) = &mut block.feeder_json {
                feeder_json["block_hash"] = Value::String(h.to_string());
            }
        }
    }
    StatusCode::OK.into_response()
}

async fn handle_write_pre_confirmed_block(
    State(state): State<SharedState>,
    Json(body): Json<Value>,
) -> Response {
    let mut state = state.lock().expect("Fake server state poisoned");
    if !state.write_pre_confirmed_block_should_succeed {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let Some(block_number) = body.get("block_number").and_then(Value::as_u64) else {
        return json_response(
            StatusCode::BAD_REQUEST,
            r#"{"error":"body missing block_number"}"#.to_string(),
        )
        .into_response();
    };
    state.pre_confirmed_block_numbers.insert(block_number);
    StatusCode::OK.into_response()
}

// Helpers

fn json_response(status: StatusCode, body: String) -> impl IntoResponse {
    (status, [(header::CONTENT_TYPE, "application/json")], body)
}

fn json_or_block_not_found(json: Option<String>) -> impl IntoResponse {
    match json {
        Some(body) => json_response(StatusCode::OK, body),
        None => json_response(StatusCode::BAD_REQUEST, BLOCK_NOT_FOUND_JSON.to_string()),
    }
}

/// Derives a feeder-gateway `get_block` JSON from an `AerospikeBlob` JSON value.
///
/// The resulting JSON matches the `BlockPostV0_13_1` wire format so that a
/// `StarknetFeederGatewayClient` (used by central sync) can deserialize it.
///
/// `block_hash` is initially `null` and is patched in later when a confirmed hash arrives via
/// `recent_block_hashes` of a subsequent blob — exactly like `state_update`.
///
/// `parent_block_hash` is extracted from `recent_block_hashes` in the same blob: the entry whose
/// `block_number == block_number - 1` carries the parent hash. Falls back to `"0x0"` if absent.
fn blob_to_feeder_block_json(blob: &Value, block_number: u64) -> Value {
    let block_info = blob.get("state_diff").and_then(|d| d.get("block_info"));

    let parent_block_hash: Value = if block_number > 0 {
        blob.get("recent_block_hashes")
            .and_then(Value::as_array)
            .and_then(|hashes| {
                hashes.iter().find(|e| {
                    e.get("block_number").and_then(Value::as_u64) == Some(block_number - 1)
                })
            })
            .and_then(|e| e.get("block_hash"))
            .cloned()
            .unwrap_or_else(|| Value::String("0x0".to_string()))
    } else {
        Value::String("0x0".to_string())
    };

    // use_kzg_da maps to l1_da_mode: BLOB (kzg) or CALLDATA.
    let l1_da_mode =
        if block_info.and_then(|bi| bi.get("use_kzg_da")).and_then(Value::as_bool).unwrap_or(false)
        {
            "BLOB"
        } else {
            "CALLDATA"
        };

    // starknet_version is Option in the blob; default to a recent known value.
    let starknet_version = block_info
        .and_then(|bi| bi.get("starknet_version"))
        .and_then(Value::as_str)
        .unwrap_or("0.14.0");

    let get = |key: &str| -> Value {
        block_info.and_then(|bi| bi.get(key)).cloned().unwrap_or(Value::Null)
    };

    serde_json::json!({
        // block_hash is null until the next blob's recent_block_hashes confirms it.
        "block_hash": null,
        "block_number": block_number,
        "parent_block_hash": parent_block_hash,
        "state_root": "0x0",
        "status": "ACCEPTED_ON_L2",
        // block_info field names match the feeder field names except block_timestamp -> timestamp.
        "sequencer_address": get("sequencer_address"),
        "timestamp": get("block_timestamp"),
        "starknet_version": starknet_version,
        "l1_da_mode": l1_da_mode,
        // CentralResourcePrice serializes as {price_in_wei, price_in_fri}, same as GasPricePerToken.
        "l1_gas_price": get("l1_gas_price"),
        "l1_data_gas_price": get("l1_data_gas_price"),
        "l2_gas_price": get("l2_gas_price"),
        // FeeMarketInfo fields map directly to the feeder's V0.14.0 additions.
        "l2_gas_consumed": blob.get("fee_market_info").and_then(|fi| fi.get("l2_gas_consumed")).cloned().unwrap_or(Value::String("0x0".to_string())),
        "next_l2_gas_price": blob.get("fee_market_info").and_then(|fi| fi.get("next_l2_gas_price")).cloned().unwrap_or(Value::String("0x0".to_string())),
        // Commitments are not available from the blob; use zero placeholders.
        "transaction_commitment": "0x0",
        "event_commitment": "0x0",
        // Transactions are omitted: state sync only needs the block header to advance.
        "transactions": [],
        "transaction_receipts": [],
    })
}

/// Converts a `CentralStateDiff` JSON value (as serialized in `AerospikeBlob`) into the feeder
/// gateway `StateUpdate` format expected by `StarknetFeederGatewayClient`.
///
/// `block_hash` is initially `null` and is patched in later when a confirmed hash arrives via
/// `recent_block_hashes` of a subsequent blob.
fn central_state_diff_to_feeder_state_update(central_state_diff: &Value) -> Value {
    // address_to_class_hash: Map<addr, hash> -> deployed_contracts: [{address, class_hash}]
    let deployed_contracts: Vec<Value> = central_state_diff
        .get("address_to_class_hash")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .map(|(addr, hash)| serde_json::json!({"address": addr, "class_hash": hash}))
                .collect()
        })
        .unwrap_or_default();

    // nonces: Map<DA-mode, Map<addr, nonce>> -> nonces: Map<addr, nonce>  (merge all DA modes)
    let nonces = central_state_diff
        .get("nonces")
        .and_then(Value::as_object)
        .map(|da_map| {
            let mut merged = serde_json::Map::new();
            for per_mode in da_map.values() {
                if let Some(addr_map) = per_mode.as_object() {
                    merged.extend(addr_map.iter().map(|(k, v)| (k.clone(), v.clone())));
                }
            }
            Value::Object(merged)
        })
        .unwrap_or(Value::Object(Default::default()));

    // storage_updates: Map<DA-mode, Map<addr, Map<key,val>>> ->
    //     storage_diffs: Map<addr, [{key,value}]>  (merge all DA modes)
    let storage_diffs = central_state_diff
        .get("storage_updates")
        .and_then(Value::as_object)
        .map(|da_map| {
            let mut merged: serde_json::Map<String, Value> = serde_json::Map::new();
            for per_mode in da_map.values() {
                if let Some(addr_map) = per_mode.as_object() {
                    for (addr, kv_map) in addr_map {
                        let storage_entries: Vec<Value> = kv_map
                            .as_object()
                            .map(|m| {
                                m.iter()
                                    .map(|(key, val)| serde_json::json!({"key": key, "value": val}))
                                    .collect()
                            })
                            .unwrap_or_default();
                        merged
                            .entry(addr.clone())
                            .or_insert_with(|| Value::Array(vec![]))
                            .as_array_mut()
                            .expect("storage_diffs entry is always an array")
                            .extend(storage_entries);
                    }
                }
            }
            Value::Object(merged)
        })
        .unwrap_or(Value::Object(Default::default()));

    // class_hash_to_compiled_class_hash: Map<hash, compiled> ->
    //     declared_classes: [{class_hash, compiled_class_hash}]
    let declared_classes: Vec<Value> = central_state_diff
        .get("class_hash_to_compiled_class_hash")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .map(|(hash, compiled)| {
                    serde_json::json!({"class_hash": hash, "compiled_class_hash": compiled})
                })
                .collect()
        })
        .unwrap_or_default();

    serde_json::json!({
        "block_hash": null,
        "new_root": "0x0",
        "old_root": "0x0",
        "state_diff": {
            "storage_diffs": storage_diffs,
            "deployed_contracts": deployed_contracts,
            "declared_classes": declared_classes,
            "migrated_compiled_classes": [],
            "old_declared_contracts": [],
            "nonces": nonces,
            "replaced_classes": [],
        }
    })
}
