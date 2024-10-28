use std::env::args;
use std::net::SocketAddr;
use std::process::exit;

use papyrus_config::validators::config_validate;
use papyrus_config::ConfigError;
use starknet_http_server::config::HttpServerConfig;
use starknet_integration_tests::integration_test_utils::{
    create_integration_test_tx_generator,
    run_transaction_generator_test_scenario,
    HttpTestClient,
};
use starknet_sequencer_infra::trace_util::configure_tracing;
use starknet_sequencer_node::config::SequencerNodeConfig;
use tracing::{error, info};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    configure_tracing();
    info!("Running integration test transaction generation for the sequencer node.");

    let tx_generator = create_integration_test_tx_generator();

    let config = SequencerNodeConfig::load_and_process(args().collect());
    if let Err(ConfigError::CommandInput(clap_err)) = config {
        clap_err.exit();
    }

    let config = config?;
    if let Err(error) = config_validate(&config) {
        error!("{}", error);
        exit(1);
    }

    let HttpServerConfig { ip, port } = config.http_server_config;
    let http_test_client = HttpTestClient::new(SocketAddr::from((ip, port)));

    let send_rpc_tx_fn = &|rpc_tx| http_test_client.assert_add_tx_success(rpc_tx);

    let n_txs = 50;
    info!("Adding {n_txs} txs.");
    run_transaction_generator_test_scenario(tx_generator, n_txs, send_rpc_tx_fn).await;

    Ok(())
}
