use std::fs::remove_dir_all;
use std::path::PathBuf;

use apollo_infra_utils::test_utils::TestIdentifier;
use clap::Parser;
use apollo_integration_tests::consts::{DATA_PREFIX_PATH, SINGLE_NODE_CONFIG_PATH};
use apollo_integration_tests::integration_test_manager::get_sequencer_setup_configs;
use apollo_integration_tests::storage::CustomPaths;
use apollo_integration_tests::utils::create_integration_test_tx_generator;
use tracing::info;

const DB_DIR: &str = "./data";

#[tokio::main]
async fn main() {
    const N_CONSOLIDATED_SEQUENCERS: usize = 1;
    const N_DISTRIBUTED_SEQUENCERS: usize = 0;
    info!("Generating system test preset for single node.");
    let args = Args::parse();

    // Creates a multi-account transaction generator for integration test
    let tx_generator = create_integration_test_tx_generator();

    let custom_paths = CustomPaths::new(
        Some(PathBuf::from(args.db_dir.clone())),
        None,
        Some(PathBuf::from(args.data_prefix_path)),
    );
    // TODO(Nadin): Align this with node_setup.
    // Run node setup.
    let (mut sequencers_setup, _node_indices) = get_sequencer_setup_configs(
        &tx_generator,
        N_CONSOLIDATED_SEQUENCERS,
        N_DISTRIBUTED_SEQUENCERS,
        Some(custom_paths),
        TestIdentifier::SystemTestDumpSingleNodeConfig,
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

    #[arg(long, default_value_t = DATA_PREFIX_PATH.to_string())]
    data_prefix_path: String,
}
