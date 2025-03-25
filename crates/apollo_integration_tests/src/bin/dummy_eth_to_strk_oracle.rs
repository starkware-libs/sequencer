use std::net::SocketAddr;

use apollo_sequencer_infra::trace_util::configure_tracing;
use apollo_integration_tests::utils::spawn_eth_to_strk_oracle_server;
use tracing::info;

const ETH_TO_STRK_ORACLE_PORT: u16 = 9000;

#[tokio::main]
async fn main() {
    configure_tracing().await;

    let socket_address = SocketAddr::from(([0, 0, 0, 0], ETH_TO_STRK_ORACLE_PORT));
    let join_handle = spawn_eth_to_strk_oracle_server(socket_address);
    info!("Spawned the dummy eth to strk oracle successfully!");

    join_handle.await.expect("The dummy eth to strk oracle has panicked!!! :(");
}
