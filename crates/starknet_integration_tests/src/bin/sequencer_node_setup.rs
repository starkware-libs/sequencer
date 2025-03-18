use std::path::PathBuf;

use clap::Parser;
use starknet_infra_utils::test_utils::TestIdentifier;
use starknet_integration_tests::integration_test_manager::{CustomPaths, IntegrationTestManager};
use starknet_integration_tests::integration_test_utils::set_panic_hook;
use starknet_sequencer_infra::trace_util::configure_tracing;
use tokio::fs::create_dir_all;
use tracing::info;

#[tokio::main]
async fn main() {
    configure_tracing().await;
    info!("Running system test setup.");

    // Parse command line arguments.
    let args = Args::parse();

    set_panic_hook();

    info!("Generate config and db files under {:?}", args.output_base_dir);

    let custom_paths = CustomPaths::new(
        Some(PathBuf::from(args.output_base_dir.clone()).join("data")),
        Some(PathBuf::from(args.output_base_dir.clone()).join("configs")),
        args.data_prefix_path.map(PathBuf::from),
    );

    let test_manager = IntegrationTestManager::new(
        args.n_consolidated,
        args.n_distributed,
        Some(custom_paths),
        TestIdentifier::PositiveFlowIntegrationTest,
    )
    .await;

    let simulator_ports_path = format!("{}/simulator_ports", args.output_base_dir);
    info!("Generate simulator ports json files under {:?}", simulator_ports_path);
    create_dir_all(&simulator_ports_path).await.unwrap();
    for (node_index, node_setup) in test_manager.get_idle_nodes().iter() {
        let path = format!("{}/node_{}", simulator_ports_path, node_index);
        node_setup.generate_simulator_ports_json(&path);
    }

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

    #[arg(long)]
    data_prefix_path: Option<String>,
}
