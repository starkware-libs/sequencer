use std::net::SocketAddr;

use apollo_infra::trace_util::configure_tracing;
use apollo_integration_tests::utils::spawn_success_recorder;
use tracing::info;

const RECORDER_PORT: u16 = 8080;

#[tokio::main]
async fn main() {
    configure_tracing(false).await;

    let socket_address = SocketAddr::from(([0, 0, 0, 0], RECORDER_PORT));
    let join_handle = spawn_success_recorder(socket_address);
    info!("Spawned the dummy success Recorder successfully!");

    join_handle.await.expect("The dummy success Recorder has panicked!!! :(");
}
