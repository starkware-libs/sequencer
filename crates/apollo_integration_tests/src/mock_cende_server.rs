mod endpoints;
mod storage;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use starknet_api::state::ThinStateDiff;
use storage::MockCendeStorage;
use tokio::task::JoinHandle;
use url::Url;

use crate::state_reader::TestClasses;

/// Spawn a mock CENDE server on the given socket address
pub(crate) fn spawn_mock_cende_server(
    socket_address: SocketAddr,
    state_diff: ThinStateDiff,
    classes: TestClasses,
) -> (Url, JoinHandle<()>) {
    let storage = Arc::new(MockCendeStorage::new());

    let join_handle = tokio::spawn(async move {
        // Initialize with genesis block
        storage.initialize_with_block_0(state_diff, classes).await;

        let router = Router::new()
            .route("/cende_recorder/write_blob", post(endpoints::write_blob))
            .route("/feeder_gateway/get_block", get(endpoints::get_block))
            .route("/feeder_gateway/get_state_update", get(endpoints::get_state_update))
            .route("/feeder_gateway/get_signature", get(endpoints::get_signature))
            .route("/feeder_gateway/is_alive", get(endpoints::is_alive))
            .route("/feeder_gateway/get_public_key", get(endpoints::get_public_key))
            .route("/feeder_gateway/get_class_by_hash", get(endpoints::get_class_by_hash))
            .route(
                "/feeder_gateway/get_compiled_class_by_class_hash",
                get(endpoints::get_compiled_class_by_class_hash),
            )
            .with_state(storage);

        axum::Server::bind(&socket_address)
            .serve(router.into_make_service())
            .await
            .expect("mock cende has panicked!");
    });

    let url = Url::parse(&format!("http://{}", socket_address)).unwrap();
    (url, join_handle)
}
