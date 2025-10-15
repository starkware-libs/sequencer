use apollo_infra_utils::tracing_utils::{configure_tracing, modify_log_level};
use clap::{Args, Parser, Subcommand};
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

const DEFAULT_DATA_PATH: &str = "/tmp/committer_storage_benchmark";
#[derive(Debug, Args)]
struct StorageArgs {
    /// Seed for the random number generator.
    #[clap(short = 's', long, default_value = "42")]
    seed: u64,
    /// Number of iterations to run the benchmark.
    #[clap(default_value = "1000")]
    n_iterations: usize,
    #[clap(long, default_value = "1000")]
    checkpoint_interval: usize,
    #[clap(long, default_value = "warn")]
    log_level: String,
    /// A path to a directory to store the output and checkpoints unless they are
    /// explicitly provided. Defaults to "/tmp/committer_storage_benchmark/".
    #[clap(short = 'd', long, default_value = None)]
    data_path: Option<String>,
    /// A path to a directory to store the csv outputs. If not given, creates a dir according to
    /// the  n_iterations (i.e., rwo runs with different n_iterations will have different csv
    /// outputs)
    #[clap(long, default_value = None)]
    output_dir: Option<String>,
    /// A path to a directory to store the checkpoints to allow benchmark recovery. If not given,
    /// creates a dir according to the n_iterations (i.e., two runs with different n_iterations
    /// will have different checkpoints)
    #[clap(long, default_value = None)]
    checkpoint_dir: Option<String>,
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
        Command::StorageBenchmark(StorageArgs {
            seed,
            n_iterations,
            checkpoint_interval,
            log_level,
            data_path,
            output_dir,
            checkpoint_dir,
        }) => {
            modify_log_level(log_level, log_filter_handle);
            let data_path = data_path.unwrap_or_else(|| DEFAULT_DATA_PATH.to_string());
            let output_dir =
                output_dir.unwrap_or_else(|| format!("{data_path}/csvs/{n_iterations}"));
            let _checkpoint_dir =
                checkpoint_dir.unwrap_or_else(|| format!("{data_path}/checkpoints/{n_iterations}"));
            run_storage_benchmark(seed, n_iterations, &output_dir, None, checkpoint_interval).await;
        }
    }
}

#[tokio::main]
async fn main() {
    let log_filter_handle = configure_tracing();
    let committer_command = CommitterCliCommand::parse();
    run_committer_cli(committer_command, log_filter_handle).await;
}
