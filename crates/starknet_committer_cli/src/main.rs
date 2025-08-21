use clap::{Args, Parser, Subcommand};
use starknet_committer_and_os_cli::tracing_utils::{configure_tracing, modify_log_level};
use starknet_committer_cli::commands::run_storage_benchmark;
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::reload::Handle;
use tracing_subscriber::Registry;

#[derive(Parser, Debug)]
pub struct CommitterCliCommand {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Args)]
struct StorageArgs {
    /// Seed for the random number generator.
    #[clap(short = 's', long, default_value = "42")]
    seed: u64,
    /// Number of iterations to run the benchmark.
    #[clap(default_value = "1000")]
    n_iterations: usize,
    #[clap(long, default_value = "warn")]
    log_level: String,
    #[clap(long, default_value = "/tmp/committer_storage_benchmark")]
    output_dir: String,
}

#[derive(Debug, Subcommand)]
enum Command {
    StorageBenchmark(StorageArgs),
}

pub async fn run_committer_cli(
    committer_command: CommitterCliCommand,
    log_filter_handle: Handle<LevelFilter, Registry>,
) {
    info!("Starting committer-cli with command: \n{:?}", committer_command);
    match committer_command.command {
        Command::StorageBenchmark(args) => {
            modify_log_level(args.log_level, log_filter_handle);
            run_storage_benchmark(args.seed, args.n_iterations, &args.output_dir).await;
        }
    }
}

#[tokio::main]
async fn main() {
    let log_filter_handle = configure_tracing();
    let committer_command = CommitterCliCommand::parse();
    run_committer_cli(committer_command, log_filter_handle).await;
}
