use clap::{Parser, Subcommand};
use starknet_api::block_hash::block_hash_calculator::{
    calculate_block_commitments,
    calculate_block_hash,
};
use tracing::info;

use crate::block_hash_cli::tests::python_tests::BlockHashPythonTestRunner;
use crate::committer_cli::block_hash::{BlockCommitmentsInput, BlockHashInput};
use crate::shared_utils::read::{load_input, write_to_file};
use crate::shared_utils::types::{run_python_test, IoArgs, PythonTestArg};

#[derive(Parser, Debug)]
pub struct BlockHashCliCommand {
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
    PythonTest(PythonTestArg),
}

pub async fn run_block_hash_cli(block_hash_cli_command: BlockHashCliCommand) {
    info!("Starting block-hash-cli with command: \n{:?}", block_hash_cli_command);
    match block_hash_cli_command.command {
        Command::BlockHash { io_args: IoArgs { input_path, output_path } } => {
            let block_hash_input: BlockHashInput = load_input(input_path);
            info!("Successfully loaded block hash input.");
            let block_hash =
                calculate_block_hash(block_hash_input.header, block_hash_input.block_commitments)
                    .unwrap_or_else(|error| panic!("Failed to calculate block hash: {error}"));
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
        Command::PythonTest(python_test_arg) => {
            run_python_test::<BlockHashPythonTestRunner>(python_test_arg).await;
        }
    }
}
