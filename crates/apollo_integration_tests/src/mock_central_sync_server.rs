use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use apollo_batcher::pre_confirmed_cende_client::RECORDER_WRITE_PRE_CONFIRMED_BLOCK_PATH;
use apollo_consensus_orchestrator::cende::RECORDER_WRITE_BLOB_PATH;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{serve, Router};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use url::Url;

pub type BlockStore = Arc<Mutex<BTreeMap<u64, serde_json::Value>>>;

pub struct MockCentralSyncServer {
    pub url: Url,
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
        .with_state(block_store);

    let handle = tokio::spawn(async move {
        let listener = TcpListener::bind(socket_address).await.unwrap();
        serve(listener, router).await.unwrap();
    });

    MockCentralSyncServer { url, _handle: handle }
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
