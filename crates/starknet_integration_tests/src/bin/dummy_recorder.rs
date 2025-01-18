use std::net::SocketAddr;

use starknet_integration_tests::utils::spawn_success_recorder;
use starknet_sequencer_infra::trace_util::configure_tracing;
use tracing::info;

const RECORDER_PORT: u16 = 8080;

#[tokio::main]
async fn main() {
    configure_tracing().await;

    let socket_address = SocketAddr::from(([0, 0, 0, 0], RECORDER_PORT));
    let join_handle = spawn_success_recorder(socket_address);
    info!("Spawned the dummy success Recorder successfully!");

    join_handle.await.expect("The dummy success Recorder has panicked!!! :(");
}
