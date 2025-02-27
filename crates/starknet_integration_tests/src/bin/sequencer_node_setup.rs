use std::path::PathBuf;

use clap::Parser;
use starknet_integration_tests::integration_test_utils::set_panic_hook;
use starknet_integration_tests::sequencer_manager::IntegrationTestManager;
use starknet_sequencer_infra::trace_util::configure_tracing;
use tracing::info;

#[tokio::main]
async fn main() {
    configure_tracing().await;
    info!("Running system test setup.");

    // Parse command line arguments.
    let args = Args::parse();

    set_panic_hook();

    info!("Generate config and db files under {:?}", args.output_base_dir);

    IntegrationTestManager::new(
        args.n_consolidated,
        args.n_distributed,
        Some(PathBuf::from(args.output_base_dir.clone()).join("data")),
        Some(PathBuf::from(args.output_base_dir.clone()).join("configs")),
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
    output_base_dir: String,
}
