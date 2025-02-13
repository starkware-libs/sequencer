use std::path::PathBuf;

use clap::Parser;
use starknet_integration_tests::config_utils::SINGLE_NODE_CONFIG_PATH;
use starknet_integration_tests::integration_test_utils::set_panic_hook;
use starknet_integration_tests::node_setup::node_setup;
use starknet_integration_tests::utils::create_integration_test_tx_generator;
use starknet_sequencer_infra::trace_util::configure_tracing;
use tracing::info;

#[tokio::main]
async fn main() {
    configure_tracing().await;
    info!("Generating system test preset for single node.");
    set_panic_hook();
    let args = Args::parse();

    // Creates a multi-account transaction generator for integration test
    let mut tx_generator = create_integration_test_tx_generator();

    // Run node setup.
    node_setup(&mut tx_generator, &args.output_path, PathBuf::from("./data")).await;
}

#[derive(Parser, Debug)]
#[command(name = "dump_single_node_config", about = "Dump single node config.")]
struct Args {
    #[arg(long,  default_value_t = SINGLE_NODE_CONFIG_PATH.to_string())]
    output_path: String,
}
