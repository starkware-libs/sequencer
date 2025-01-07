use starknet_integration_tests::utils::spawn_success_recorder;
use starknet_sequencer_infra::trace_util::configure_tracing;
use tracing::info;

#[tokio::main]
async fn main() {
    configure_tracing().await;

    let port = papyrus_common::tcp::find_free_port();
    info!("Running a mock success Recorder at port: {port}");

    let (url, join_handle) = spawn_success_recorder(port);
    info!("Spawned success Recorder at URL: {url}");

    join_handle.await.expect("The mock success Recorder has panicked!!! :(");
}
