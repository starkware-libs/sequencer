use crate::tests::python_tests::PythonTest;
use clap::{Args, Parser, Subcommand};
use committer::block_committer::commit::commit_block;
use filled_tree_output::filled_forest::SerializedForest;
use parse_input::read::parse_input;
use std::io;

pub mod filled_tree_output;
pub mod parse_input;
pub mod tests;

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
    /// Given previous state tree skeleton and a state diff, computes the new commitment.
    Commit {
        /// File path to input.
        #[clap(long, short = 'i', default_value = "stdin")]
        input_path: String,

        /// File path to output.
        #[clap(long, short = 'o', default_value = "stdout")]
        output_path: String,
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
struct GlobalOptions {}

#[tokio::main]
/// Main entry point of the committer CLI.
async fn main() {
    let args = CommitterCliArgs::parse();

    match args.command {
        Command::Commit {
            input_path: _input_path,
            output_path: _output_path,
        } => {
            // TODO(Nimrod, 20/6/2024): Allow read/write from file path.
            let input =
                parse_input(io::read_to_string(io::stdin()).expect("Failed to read from stdin."))
                    .expect("Failed to parse the given input.");
            let serialized_filled_forest = SerializedForest(
                commit_block(input)
                    .await
                    .expect("Failed to commit the given block."),
            );
            serialized_filled_forest
                .forest_to_python()
                .expect("Failed to print new facts to python.");
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
    }
}
