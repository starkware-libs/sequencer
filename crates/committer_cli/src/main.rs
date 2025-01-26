use clap::{Args, Parser, Subcommand};
use committer_cli::block_hash::{BlockCommitmentsInput, BlockHashInput};
use committer_cli::commands::parse_and_commit;
use committer_cli::parse_input::read::{load_input, read_input, write_to_file};
use committer_cli::tests::python_tests::PythonTest;
use committer_cli::tracing_utils::configure_tracing;
use starknet_api::block_hash::block_hash_calculator::{
    calculate_block_commitments,
    calculate_block_hash,
};
use tracing::info;

/// Committer CLI.
#[derive(Debug, Parser)]
#[clap(name = "committer-cli", version)]
pub struct CommitterCliArgs {
    #[clap(flatten)]
    global_options: GlobalOptions,

    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Args)]
pub struct IoArgs {
    /// File path to input.
    #[clap(long, short = 'i')]
    input_path: String,

    /// File path to output.
    #[clap(long, short = 'o', default_value = "stdout")]
    output_path: String,
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

#[derive(Debug, Args)]
struct GlobalOptions {}

#[tokio::main]
/// Main entry point of the committer CLI.
async fn main() {
    // Initialize the logger. The log_filter_handle is used to change the log level. The
    // default log level is INFO.
    let log_filter_handle = configure_tracing();

    let args = CommitterCliArgs::parse();
    info!("Starting committer-cli with args: \n{:?}", args);

    match args.command {
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
