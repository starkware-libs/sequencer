use std::net::SocketAddr;

use apollo_infra::trace_util::configure_tracing;
use apollo_integration_tests::utils::spawn_fake_recorder;
use tracing::info;

const RECORDER_PORT: u16 = 8080;

#[tokio::main]
async fn main() {
    configure_tracing().await;

    let mut fake_recorder =
        spawn_fake_recorder(SocketAddr::from(([0, 0, 0, 0], RECORDER_PORT))).await;
    info!("Spawned the dummy fake Recorder successfully!");

    fake_recorder.run_until_exit().await;
}
