use std::fs::remove_dir_all;
use std::path::PathBuf;

use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::consts::{DATA_PREFIX_PATH, SINGLE_NODE_CONFIG_PATH};
use apollo_integration_tests::integration_test_manager::IntegrationTestManager;
use apollo_integration_tests::storage::CustomPaths;
use clap::Parser;
use tracing::info;

const DB_DIR: &str = "./data";

#[tokio::main]
async fn main() {
    const N_CONSOLIDATED_SEQUENCERS: usize = 1;
    const N_DISTRIBUTED_SEQUENCERS: usize = 0;
    info!("Generating system test preset for single node.");
    let args = Args::parse();

    let custom_paths = CustomPaths::new(
        Some(PathBuf::from(args.db_dir.clone())),
        None,
        Some(PathBuf::from(args.data_prefix_path)),
    );

    let test_manager = IntegrationTestManager::new(
        N_CONSOLIDATED_SEQUENCERS,
        N_DISTRIBUTED_SEQUENCERS,
        Some(custom_paths),
        TestIdentifier::SystemTestDumpSingleNodeConfig,
    )
    .await;

    test_manager.get_idle_nodes().iter().for_each(|(_, node_setup)| {
        node_setup.get_executables().iter().for_each(|executable_setup| {
            executable_setup.dump_config_file_changes();
        });
    });

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
