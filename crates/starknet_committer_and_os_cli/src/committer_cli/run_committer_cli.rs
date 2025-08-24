use clap::{Parser, Subcommand};
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::reload::Handle;
use tracing_subscriber::Registry;

use crate::committer_cli::commands::{parse_and_commit, run_storage_benchmark};
use crate::committer_cli::tests::python_tests::CommitterPythonTestRunner;
use crate::shared_utils::types::{run_python_test, IoArgs, PythonTestArg};

#[derive(Parser, Debug)]
pub struct CommitterCliCommand {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Given previous state tree skeleton and a state diff, computes the new commitment.
    Commit {
        #[clap(flatten)]
        io_args: IoArgs,
    },
    PythonTest(PythonTestArg),
    /// Run the committer on random data with owned storage.
    StorageBenchmark,
}

pub async fn run_committer_cli(
    committer_command: CommitterCliCommand,
    log_filter_handle: Handle<LevelFilter, Registry>,
) {
    info!("Starting committer-cli with command: \n{:?}", committer_command);
    match committer_command.command {
        Command::Commit { io_args: IoArgs { input_path, output_path } } => {
            parse_and_commit(input_path, output_path, log_filter_handle).await;
        }

        Command::PythonTest(python_test_arg) => {
            run_python_test::<CommitterPythonTestRunner>(python_test_arg).await;
        }
        Command::StorageBenchmark => {
            run_storage_benchmark().await;
        }
    }
}
