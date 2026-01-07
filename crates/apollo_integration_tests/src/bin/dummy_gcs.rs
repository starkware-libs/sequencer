use std::net::SocketAddr;

use apollo_infra::trace_util::configure_tracing;
use apollo_integration_tests::utils::spawn_dummy_gcs_server;
use tracing::info;

const GCS_PORT: u16 = 9001;

#[tokio::main]
async fn main() {
    configure_tracing().await;

    let socket_address = SocketAddr::from(([0, 0, 0, 0], GCS_PORT));
    let join_handle = spawn_dummy_gcs_server(socket_address);
    info!("Spawned the dummy GCS server successfully!");

    join_handle.await.expect("The dummy GCS server has panicked!!! :(");
}
