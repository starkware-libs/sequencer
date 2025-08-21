use clap::{Args, Parser, Subcommand};
use starknet_committer_and_os_cli::tracing_utils::configure_tracing;
use starknet_committer_cli::commands::run_storage_benchmark;
use tracing::level_filters::LevelFilter;
use tracing::{info, Level};
use tracing_subscriber::reload::Handle;
use tracing_subscriber::Registry;

#[derive(Parser, Debug)]
pub struct CommitterCliCommand {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Args)]
struct StorageArgs {
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
            let level = match args.log_level.to_lowercase().as_str() {
                "error" => Level::ERROR,
                "warn" => Level::WARN,
                "info" => Level::INFO,
                "debug" => Level::DEBUG,
                "trace" => Level::TRACE,
                _ => Level::INFO,
            };
            log_filter_handle
                .modify(|filter| *filter = level.into())
                .expect("Failed to set the log level.");
            run_storage_benchmark(args.n_iterations, &args.output_dir).await;
        }
    }
}

#[tokio::main]
async fn main() {
    let log_filter_handle = configure_tracing();
    let committer_command = CommitterCliCommand::parse();
    run_committer_cli(committer_command, log_filter_handle).await;
}
