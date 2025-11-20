use apollo_infra_utils::tracing_utils::{configure_tracing, modify_log_level};
use clap::{Parser, Subcommand};
use starknet_committer_cli::args::{GlobalArgs, StorageBenchmarkCommand};
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
    #[clap(subcommand)]
    StorageBenchmark(StorageBenchmarkCommand),
}

pub async fn run_committer_cli(
    committer_command: CommitterCliCommand,
    log_filter_handle: Handle<LevelFilter, Registry>,
) {
    info!("Starting committer-cli with command: \n{:?}", committer_command);
    match committer_command.command {
        Command::StorageBenchmark(storage_benchmark_args) => {
            let GlobalArgs { ref log_level, .. } = storage_benchmark_args.global_args();
            modify_log_level(log_level.clone(), log_filter_handle);
            storage_benchmark_args.run_benchmark().await;
        }
    }
}

#[tokio::main]
async fn main() {
    let log_filter_handle = configure_tracing();
    let committer_command = CommitterCliCommand::parse();
    run_committer_cli(committer_command, log_filter_handle).await;
}
