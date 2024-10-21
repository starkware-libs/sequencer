use std::env::args;
use std::net::SocketAddr;
use std::process::exit;

use papyrus_config::validators::config_validate;
use papyrus_config::ConfigError;
use starknet_http_server::config::HttpServerConfig;
use starknet_integration_tests::integration_test_utils::{
    create_integration_test_tx_generator,
    send_rpc_tx,
    HttpTestClient,
};
use starknet_mempool_infra::trace_util::configure_tracing;
use starknet_mempool_node::config::SequencerNodeConfig;
use tracing::{error, info};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    configure_tracing();
    info!("Running integration test transaction generation for the sequencer node.");

    let mut tx_generator = create_integration_test_tx_generator();

    let config = SequencerNodeConfig::load_and_process(args().collect());
    if let Err(ConfigError::CommandInput(clap_err)) = config {
        clap_err.exit();
    }

    let config = config?;
    if let Err(error) = config_validate(&config) {
        error!("{}", error);
        exit(1);
    }

    let account0_invoke_nonce1 = tx_generator.account_with_id(0).generate_invoke_with_tip(1);
    let account0_invoke_nonce2 = tx_generator.account_with_id(0).generate_invoke_with_tip(1);
    let account1_invoke_nonce1 = tx_generator.account_with_id(1).generate_invoke_with_tip(1);

    let HttpServerConfig { ip, port } = config.http_server_config;
    let http_test_client = HttpTestClient::new(SocketAddr::from((ip, port)));

    let account0_invoke_nonce1_tx_hash =
        send_rpc_tx(account0_invoke_nonce1, |tx| http_test_client.assert_add_tx_success(tx)).await;
    let account1_invoke_nonce1_tx_hash =
        send_rpc_tx(account1_invoke_nonce1, |tx| http_test_client.assert_add_tx_success(tx)).await;
    let account0_invoke_nonce2_tx_hash =
        send_rpc_tx(account0_invoke_nonce2, |tx| http_test_client.assert_add_tx_success(tx)).await;

    info!("Add tx result: {:?}", account0_invoke_nonce1_tx_hash);
    info!("Add tx result: {:?}", account1_invoke_nonce1_tx_hash);
    info!("Add tx result: {:?}", account0_invoke_nonce2_tx_hash);

    Ok(())
}
