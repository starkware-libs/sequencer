use clap::{Args, Parser, Subcommand};
use committer_cli::block_hash::{BlockCommitmentsInput, BlockHashInput};
use committer_cli::commands::commit;
use committer_cli::parse_input::read::{load_from_file, write_to_file};
use committer_cli::tests::python_tests::PythonTest;
use starknet_api::block_hash::block_hash_calculator::{
    calculate_block_commitments, calculate_block_hash,
};
use std::io;

/// Committer CLI.
#[derive(Debug, Parser)]
#[clap(name = "committer-cli", version)]
pub struct CommitterCliArgs {
    #[clap(flatten)]
    global_options: GlobalOptions,

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
    /// Given previous state tree skeleton and a state diff, computes the new commitment.
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
        /// Test name.
        #[clap(long)]
        test_name: String,

        /// Test inputs as a json.
        #[clap(long)]
        inputs: Option<String>,
    },
}

#[derive(Debug, Args)]
struct IoArgs {
    /// File path to input.
    #[clap(long, short = 'i', default_value = "stdin")]
    input_path: String,

    /// File path to output.
    #[clap(long, short = 'o', default_value = "stdout")]
    output_path: String,
}

#[derive(Debug, Args)]
struct GlobalOptions {}

#[tokio::main]
/// Main entry point of the committer CLI.
async fn main() {
    let args = CommitterCliArgs::parse();

    match args.command {
        Command::Commit { io_args: _ } => {
            // TODO(Nimrod, 20/6/2024): Allow read/write from file path.
            let input_string = io::read_to_string(io::stdin()).expect("Failed to read from stdin.");
            commit(&input_string).await;
        }

        Command::PythonTest { test_name, inputs } => {
            // Create PythonTest from test_name.
            let test = PythonTest::try_from(test_name)
                .unwrap_or_else(|error| panic!("Failed to create PythonTest: {}", error));

            // Run relevant test.
            let output = test
                .run(inputs.as_deref())
                .await
                .unwrap_or_else(|error| panic!("Failed to run test: {}", error));

            // Print test's output.
            print!("{}", output);
        }

        Command::BlockHash { io_args } => {
            let block_hash_input: BlockHashInput = load_from_file(&io_args.input_path);
            let block_hash = serde_json::to_string(&calculate_block_hash(
                block_hash_input.header,
                block_hash_input.block_commitments,
            ))
            .expect("Failed to serialize block hash");
            write_to_file(&io_args.output_path, &block_hash);
        }

        Command::BlockHashCommitments { io_args } => {
            let commitments_input: BlockCommitmentsInput = load_from_file(&io_args.input_path);
            let commitments = serde_json::to_string(&calculate_block_commitments(
                &commitments_input.transactions_data,
                &commitments_input.state_diff,
                commitments_input.l1_da_mode,
            ))
            .expect("Failed to serialize commitments");
            write_to_file(&io_args.output_path, &commitments);
        }
    }
}
