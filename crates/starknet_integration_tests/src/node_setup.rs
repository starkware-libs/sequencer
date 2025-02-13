use std::path::PathBuf;

use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use tokio::fs::rename;
use tracing::info;

use crate::sequencer_manager::{get_sequencer_setup_configs, NodeSetup};

pub async fn node_setup(
    tx_generator: &mut MultiAccountTransactionGenerator,
    config_path: &str,
    base_db_path_dir: PathBuf,
) -> Vec<NodeSetup> {
    const N_CONSOLIDATED_SEQUENCERS: usize = 1;
    const N_DISTRIBUTED_SEQUENCERS: usize = 0;
    info!("Node setup");

    // Get the sequencer configurations.
    let sequencers_setup = get_sequencer_setup_configs(
        tx_generator,
        N_CONSOLIDATED_SEQUENCERS,
        N_DISTRIBUTED_SEQUENCERS,
        Some(base_db_path_dir),
    )
    .await;

    // There's one node with one executable.
    let original_config_path = sequencers_setup[0].get_executables()[0].node_config_path.clone();
    let new_config_path = PathBuf::from(config_path);

    // Move (rename) the file to the current directory with the new name
    rename(&original_config_path, &new_config_path).await.expect("Failed to move node config file");
    println!("Config file moved from {:?} to {:?}", original_config_path, new_config_path);
    sequencers_setup
}

// TODO(Nadin): Improve the argument parsing.
pub fn get_base_db_path(args: Vec<String>) -> PathBuf {
    let arg_name = "--base_db_path_dir";
    match args.as_slice() {
        [] => PathBuf::from("./data"),
        [arg, path] if arg == arg_name => PathBuf::from(path),
        _ => {
            eprintln!("Error: Bad argument. The only allowed argument is '{}'.", arg_name);
            std::process::exit(1);
        }
    }
}
