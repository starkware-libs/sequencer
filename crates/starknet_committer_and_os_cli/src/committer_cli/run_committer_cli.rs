use clap::{Parser, Subcommand};
use starknet_api::block_hash::block_hash_calculator::{
    calculate_block_commitments,
    calculate_block_hash,
};
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::reload::Handle;
use tracing_subscriber::Registry;

use crate::committer_cli::block_hash::{BlockCommitmentsInput, BlockHashInput};
use crate::committer_cli::commands::parse_and_commit;
use crate::committer_cli::tests::python_tests::PythonTest;
use crate::shared_utils::read::{load_input, read_input, write_to_file};
use crate::shared_utils::types::IoArgs;

#[derive(Parser, Debug)]
pub struct CommitterCliCommand {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Calculates the block hash.
    BlockHash {
        #[clap(flatten)]
        io_args: IoArgs,
    },
    /// Calculates commitments needed for the block hash.
    BlockHashCommitments {
        #[clap(flatten)]
        io_args: IoArgs,
    },
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

        Command::BlockHash { io_args: IoArgs { input_path, output_path } } => {
            let block_hash_input: BlockHashInput = load_input(input_path);
            info!("Successfully loaded block hash input.");
            let block_hash =
                calculate_block_hash(block_hash_input.header, block_hash_input.block_commitments)
                    .unwrap_or_else(|error| panic!("Failed to calculate block hash: {}", error));
            write_to_file(&output_path, &block_hash);
            info!("Successfully computed block hash {:?}.", block_hash);
        }

        Command::BlockHashCommitments { io_args: IoArgs { input_path, output_path } } => {
            let commitments_input: BlockCommitmentsInput = load_input(input_path);
            info!("Successfully loaded block hash commitment input.");
            let commitments = calculate_block_commitments(
                &commitments_input.transactions_data,
                &commitments_input.state_diff,
                commitments_input.l1_da_mode,
                &commitments_input.starknet_version,
            );
            write_to_file(&output_path, &commitments);
            info!("Successfully computed block hash commitment: \n{:?}", commitments);
        }
    }
}
