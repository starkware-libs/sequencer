use std::env::args;
use std::path::PathBuf;

use starknet_integration_tests::node_setup::node_setup;
use starknet_integration_tests::utils::create_integration_test_tx_generator;
use starknet_sequencer_infra::trace_util::configure_tracing;
use tracing::info;

#[tokio::main]
async fn main() {
    configure_tracing().await;
    info!("Running system test setup.");

    // Parse command line arguments.
    let args: Vec<String> = args().skip(1).collect();
    let base_db_path = get_base_db_path(args);

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
    // Keep the sequenser_setups in a variable to avoid dropping it.
    let _sequencer_setups =
        node_setup(&mut tx_generator, "./single_node_config.json", base_db_path).await;
}

// TODO(Nadin): Improve the argument parsing.
fn get_base_db_path(args: Vec<String>) -> PathBuf {
    let arg_name = "--base_db_path_dir";
    match args.as_slice() {
        [arg, path] if arg == arg_name => PathBuf::from(path),
        _ => {
            eprintln!(
                "Error: Missing or incorrect argument. The only allowed argument is '{}'.",
                arg_name
            );
            std::process::exit(1);
        }
    }
}
