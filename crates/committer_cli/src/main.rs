use crate::tests::python_tests::PythonTest;
use clap::{Args, Parser, Subcommand};
use std::path::Path;

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

/// Main entry point of the committer CLI.
fn main() {
    let args = CommitterCliArgs::parse();

    match args.command {
        Command::Commit {
            input_path,
            output_path,
        } => {
            let input_file_name = Path::new(&input_path);
            let output_file_name = Path::new(&output_path);
            assert!(
                input_file_name.is_absolute() && output_file_name.is_absolute(),
                "Given paths must be absolute."
            );

            // Business logic to be implemented here.
            let output = std::fs::read(input_file_name)
                .unwrap_or_else(|_| panic!("Failed to read input from file '{input_file_name:?}'"));

            // Output to file.
            std::fs::write(output_file_name, output).expect("Failed to write output");
        }

        Command::PythonTest { test_name, inputs } => {
            // Create PythonTest from test_name.
            let test = PythonTest::try_from(test_name)
                .unwrap_or_else(|error| panic!("Failed to create PythonTest: {}", error));

            // Run relevant test.
            let output = test
                .run(inputs.as_deref())
                .unwrap_or_else(|error| panic!("Failed to run test: {}", error));

            // Print test's output.
            print!("{}", output);
        }
    }
}
