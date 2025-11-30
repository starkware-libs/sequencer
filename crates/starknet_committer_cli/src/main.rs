use apollo_infra_utils::tracing_utils::{configure_tracing, modify_log_level};
use clap::{Args, Parser, Subcommand};
use starknet_committer_cli::args::{default_preset, GlobalArgs, Preset};
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
            let args = default_preset(preset);
            let GlobalArgs { ref log_level, .. } = args.global_args();
            modify_log_level(log_level.clone(), log_filter_handle);
            args.run_benchmark().await;
        }
    }
}

#[tokio::main]
async fn main() {
    let log_filter_handle = configure_tracing();
    let committer_command = CommitterCliCommand::parse();
    run_committer_cli(committer_command, log_filter_handle).await;
}
