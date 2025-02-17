use std::path::PathBuf;

use clap::Parser;
use starknet_integration_tests::integration_test_utils::set_panic_hook;
use starknet_integration_tests::sequencer_manager::get_sequencer_setup_configs;
use starknet_integration_tests::utils::create_integration_test_tx_generator;
use starknet_sequencer_infra::trace_util::configure_tracing;
use tracing::info;

#[tokio::main]
async fn main() {
    configure_tracing().await;
    info!("Running system test setup.");

    // Parse command line arguments.
    let args = Args::parse();

    set_panic_hook();

    // Creates a multi-account transaction generator for integration test
    let tx_generator = create_integration_test_tx_generator();

    info!("Generate config files under {:?}", args.configs_dir);
    // Run node setup.
    get_sequencer_setup_configs(
        &tx_generator,
        args.n_consolidated,
        args.n_distributed,
        Some(PathBuf::from(args.db_dir)),
        Some(PathBuf::from(args.configs_dir)),
    )
    .await;

    info!("Node setup completed");
}

#[derive(Parser, Debug)]
#[command(name = "node_setup", about = "Generate sequencer db and config files.")]
struct Args {
    #[arg(long)]
    n_consolidated: usize,

    #[arg(long)]
    n_distributed: usize,

    #[arg(long)]
    configs_dir: String,

    #[arg(long)]
    db_dir: String,
}
