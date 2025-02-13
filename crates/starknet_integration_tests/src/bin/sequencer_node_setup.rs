use std::path::PathBuf;

use clap::Parser;
use starknet_integration_tests::node_setup::node_setup;
use starknet_integration_tests::utils::create_integration_test_tx_generator;
use starknet_sequencer_infra::trace_util::configure_tracing;
use tracing::info;

#[tokio::main]
async fn main() {
    configure_tracing().await;
    info!("Running system test setup.");

    // Parse command line arguments.
    let args = Args::parse();

    // TODO(Tsabary): remove the hook definition once we transition to proper usage of task
    // spawning.
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_panic(info);
        std::process::exit(1);
    }));

    // Creates a multi-account transaction generator for integration test
    let mut tx_generator = create_integration_test_tx_generator();

    // Run node setup.
    node_setup(&mut tx_generator, "./single_node_config.json", PathBuf::from(args.db_dir)).await;
}

#[derive(Parser, Debug)]
#[command(name = "node_setup", about = "Generate sequencer db and config files.")]
struct Args {
    #[arg(long)]
    db_dir: String,
}
