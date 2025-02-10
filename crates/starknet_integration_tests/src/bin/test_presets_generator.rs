use std::path::PathBuf;

use clap::Parser;
use starknet_integration_tests::sequencer_manager::get_sequencer_setup_configs;
use starknet_integration_tests::utils::create_integration_test_tx_generator;
use tracing::info;

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let tx_generator = create_integration_test_tx_generator();

    info!("Generate config files under {:?}", args.preset_dir);
    let (_sequencers_setup, _node_indices) = get_sequencer_setup_configs(
        &tx_generator,
        args.n_consolidated,
        args.n_distributed,
        Some(PathBuf::from(args.db_dir)),
        Some(PathBuf::from(args.preset_dir)),
    )
    .await;

    info!("Finished generating config files.");
}

#[derive(Parser, Debug)]
#[command(name = "test_presets_generator", about = "Generate sequencer config files.")]
struct Args {
    #[arg(long)]
    n_consolidated: usize,

    #[arg(long)]
    n_distributed: usize,

    #[arg(long)]
    preset_dir: String,

    #[arg(long)]
    db_dir: String,
}
