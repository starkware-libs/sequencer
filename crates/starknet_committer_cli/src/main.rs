use apollo_infra_utils::tracing_utils::configure_tracing;
use clap::{Args, Parser, Subcommand};
use starknet_committer_cli::commands::run_benchmark;
use starknet_committer_cli::presets::types::Preset;
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::reload::Handle;
use tracing_subscriber::Registry;

#[derive(Parser, Debug)]
pub struct CommitterCliCommand {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    StorageBenchmark(StorageBenchmarkArgs),
}

#[derive(Args, Debug)]
struct StorageBenchmarkArgs {
    /// The preset to use for the storage benchmark.
    #[clap(long)]
    preset: Preset,
}

pub async fn run_committer_cli(
    committer_command: CommitterCliCommand,
    log_filter_handle: Handle<LevelFilter, Registry>,
) {
    info!("Starting committer-cli with command: \n{:?}", committer_command);
    match committer_command.command {
        Command::StorageBenchmark(StorageBenchmarkArgs { preset }) => {
            let fields = preset.preset_fields();
            log_filter_handle
                .modify(|filter| *filter = fields.flavor_fields().log_level.into())
                .expect("Failed to set the log level.");
            run_benchmark(fields).await;
        }
    }
}

#[tokio::main]
async fn main() {
    let log_filter_handle = configure_tracing();
    let committer_command = CommitterCliCommand::parse();
    run_committer_cli(committer_command, log_filter_handle).await;
}
