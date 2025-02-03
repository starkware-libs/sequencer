use clap::{Parser, Subcommand};
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::reload::Handle;
use tracing_subscriber::Registry;

use crate::committer_cli::commands::parse_and_commit;
use crate::committer_cli::tests::python_tests::PythonTest;
use crate::shared_utils::read::{read_input, write_to_file};
use crate::shared_utils::types::IoArgs;

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
    PythonTest {
        #[clap(flatten)]
        io_args: IoArgs,

        /// Test name.
        #[clap(long)]
        test_name: String,
    },
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

        Command::PythonTest { io_args: IoArgs { input_path, output_path }, test_name } => {
            // Create PythonTest from test_name.
            let test = PythonTest::try_from(test_name)
                .unwrap_or_else(|error| panic!("Failed to create PythonTest: {}", error));
            let input = read_input(input_path);

            // Run relevant test.
            let output = test
                .run(Some(&input))
                .await
                .unwrap_or_else(|error| panic!("Failed to run test: {}", error));

            // Write test's output.
            write_to_file(&output_path, &output);
        }
    }
}
