use std::path::PathBuf;

use apollo_infra::trace_util::configure_tracing;
use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::integration_test_manager::IntegrationTestManager;
use apollo_integration_tests::storage::CustomPaths;
use clap::Parser;
use tokio::fs::create_dir_all;
use tracing::info;

#[tokio::main]
async fn main() {
    configure_tracing().await;
    info!("Running system test setup.");

    // Parse command line arguments.
    let args = Args::parse();

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
        // TODO(Tsabary/Nadin): add a different identifier.
        TestIdentifier::PositiveFlowIntegrationTest,
    )
    .await;

    // TODO(Tsabary/Nadin): rename dir from "ports" to "config".
    // TODO(Tsabary/Nadin): avoid the hard-coded file names, e.g., "node_"
    let simulator_config_file = format!("{}/simulator_ports", args.output_base_dir);
    info!("Generate simulator ports json files under {:?}", simulator_config_file);
    create_dir_all(&simulator_config_file).await.unwrap();
    for (node_index, node_setup) in test_manager.get_idle_nodes().iter() {
        let path = format!("{}/node_{}", simulator_config_file, node_index);
        node_setup.generate_simulator_ports_json(&path);
    }

    info!("Node setup completed");
}

#[derive(Parser, Debug)]
#[command(
    name = "node_setup",
    about = "Generate sequencer and simulator testing db and config files."
)]
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
