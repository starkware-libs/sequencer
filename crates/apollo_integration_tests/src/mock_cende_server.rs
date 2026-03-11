use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use apollo_batcher::pre_confirmed_cende_client::RECORDER_WRITE_PRE_CONFIRMED_BLOCK_PATH;
use apollo_consensus_orchestrator::cende::{AerospikeBlob, RECORDER_WRITE_BLOB_PATH};
use apollo_starknet_client::reader::objects::block::BlockSignatureData;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{serve, Json, Router};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockNumber};
use starknet_types_core::felt::Felt;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use url::Url;

type BlobStore = Arc<RwLock<BTreeMap<BlockNumber, AerospikeBlob>>>;

const FEEDER_GATEWAY_IS_ALIVE_PATH: &str = "/feeder_gateway/is_alive";
const FEEDER_GATEWAY_ALIVE_RESPONSE: &str = "FeederGateway is alive!";
const FEEDER_GATEWAY_GET_BLOCK_PATH: &str = "/feeder_gateway/get_block";
const FEEDER_GATEWAY_GET_SIGNATURE_PATH: &str = "/feeder_gateway/get_signature";
const FEEDER_GATEWAY_GET_PUBLIC_KEY_PATH: &str = "/feeder_gateway/get_public_key";

#[derive(Serialize)]
struct StarknetErrorResponse {
    code: &'static str,
    message: &'static str,
}

fn block_not_found() -> impl IntoResponse {
    (
        StatusCode::BAD_REQUEST,
        Json(StarknetErrorResponse {
            code: "StarknetErrorCode.BLOCK_NOT_FOUND",
            message: "Block not found",
        }),
    )
}

async fn handle_write_blob(
    State(blobs): State<BlobStore>,
    Json(blob): Json<AerospikeBlob>,
) -> StatusCode {
    blobs.write().await.insert(blob.block_number, blob);
    StatusCode::OK
}

async fn handle_write_pre_confirmed_block() -> StatusCode {
    StatusCode::OK
}

async fn handle_is_alive() -> &'static str {
    FEEDER_GATEWAY_ALIVE_RESPONSE
}

async fn handle_get_public_key() -> impl IntoResponse {
    // SequencerPublicKey(PublicKey(Felt)) serializes as a hex string.
    Json("0x0")
}

#[derive(Deserialize)]
struct BlockNumberQuery {
    #[serde(rename = "blockNumber")]
    block_number: String,
}

async fn handle_get_signature(
    State(blobs): State<BlobStore>,
    Query(params): Query<BlockNumberQuery>,
) -> impl IntoResponse {
    let Ok(block_number) = params.block_number.parse::<u64>() else {
        return block_not_found().into_response();
    };
    let blobs = blobs.read().await;
    let Some(blob) = blobs.get(&BlockNumber(block_number)) else {
        return block_not_found().into_response();
    };
    let signature_data = BlockSignatureData::V0_13_2 {
        block_hash: BlockHash(blob.proposal_commitment.0),
        signature: [Felt::ZERO, Felt::ZERO],
    };
    Json(signature_data).into_response()
}

async fn handle_get_block(
    State(blobs): State<BlobStore>,
    Query(params): Query<BlockNumberQuery>,
) -> impl IntoResponse {
    if params.block_number != "latest" {
        return block_not_found().into_response();
    }
    let blobs = blobs.read().await;
    let Some((&block_number, blob)) = blobs.iter().next_back() else {
        return block_not_found().into_response();
    };
    let block_hash_and_number =
        BlockHashAndNumber { hash: BlockHash(blob.proposal_commitment.0), number: block_number };
    Json(block_hash_and_number).into_response()
}

pub fn spawn_mock_cende_server(port: u16) -> (Url, JoinHandle<()>) {
    let socket_address = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port);
    let url = Url::parse(&format!("http://{socket_address}")).expect("Failed to parse URL");
    let blobs: BlobStore = Arc::new(RwLock::new(BTreeMap::new()));

    let router = Router::new()
        .route(RECORDER_WRITE_BLOB_PATH, post(handle_write_blob))
        .route(RECORDER_WRITE_PRE_CONFIRMED_BLOCK_PATH, post(handle_write_pre_confirmed_block))
        .route(FEEDER_GATEWAY_IS_ALIVE_PATH, get(handle_is_alive))
        .route(FEEDER_GATEWAY_GET_PUBLIC_KEY_PATH, get(handle_get_public_key))
        .route(FEEDER_GATEWAY_GET_SIGNATURE_PATH, get(handle_get_signature))
        .route(FEEDER_GATEWAY_GET_BLOCK_PATH, get(handle_get_block))
        .with_state(blobs);

    let join_handle = tokio::spawn(async move {
        let listener = TcpListener::bind(socket_address).await.expect("Failed to bind");
        serve(listener, router).await.expect("Mock cende server failed");
    });

    (url, join_handle)
}
