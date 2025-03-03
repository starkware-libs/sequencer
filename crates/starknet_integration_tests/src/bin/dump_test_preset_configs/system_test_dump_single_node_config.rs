use std::fs::remove_dir_all;
use std::path::PathBuf;

use clap::Parser;
use starknet_integration_tests::consts::SINGLE_NODE_CONFIG_PATH;
use starknet_integration_tests::integration_test_utils::set_panic_hook;
use starknet_integration_tests::sequencer_manager::get_sequencer_setup_configs;
use starknet_integration_tests::utils::create_integration_test_tx_generator;
use starknet_sequencer_infra::trace_util::configure_tracing;
use tempfile::TempDir;
use tracing::info;

const DB_DIR: &str = "./data";

#[tokio::main]
async fn main() {
    const N_CONSOLIDATED_SEQUENCERS: usize = 1;
    const N_DISTRIBUTED_SEQUENCERS: usize = 0;
    configure_tracing().await;
    info!("Generating system test preset for single node.");
    set_panic_hook();
    let args = Args::parse();

    // Creates a multi-account transaction generator for integration test
    let tx_generator = create_integration_test_tx_generator();

    let temp_dir = TempDir::new().unwrap();
    let temp_dir_path = temp_dir.path().to_path_buf();

    // Run node setup.
    let (mut sequencers_setup, _node_indices) = get_sequencer_setup_configs(
        &tx_generator,
        N_CONSOLIDATED_SEQUENCERS,
        N_DISTRIBUTED_SEQUENCERS,
        Some(PathBuf::from(args.db_dir.clone())),
        Some(temp_dir_path),
        args.data_prefix_path.map(PathBuf::from),
    )
    .await;

    // Dump the config file in the single node path.
    let single_node_path = PathBuf::from(args.config_output_path);
    sequencers_setup[0].set_executable_config_path(0, single_node_path).unwrap();
    sequencers_setup[0].get_executables()[0].dump_config_file_changes();

    remove_dir_all(args.db_dir).expect("Failed to remove db directory");

    info!("System test preset for single node generated successfully.");
}

#[derive(Parser, Debug)]
#[command(name = "system_test_dump_single_node_config", about = "Dump single node config.")]
struct Args {
    #[arg(long,  default_value_t = SINGLE_NODE_CONFIG_PATH.to_string())]
    config_output_path: String,

    #[arg(long,  default_value_t = DB_DIR.to_string())]
    db_dir: String,

    #[arg(long)]
    data_prefix_path: Option<String>,
}
