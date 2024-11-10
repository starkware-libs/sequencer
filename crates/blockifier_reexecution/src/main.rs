use blockifier_reexecution::state_reader::test_state_reader::{
    ConsecutiveTestStateReaders,
    OfflineConsecutiveStateReaders,
};
use blockifier_reexecution::state_reader::utils::{
    get_block_numbers_for_reexecution,
    reexecute_and_verify_correctness,
    write_block_reexecution_data_to_file,
    JSON_RPC_VERSION,
};
use clap::{Args, Parser, Subcommand};
use starknet_api::block::BlockNumber;
use starknet_gateway::config::RpcStateReaderConfig;

/// BlockifierReexecution CLI.
#[derive(Debug, Parser)]
#[clap(name = "blockifier-reexecution-cli", version)]
pub struct BlockifierReexecutionCliArgs {
    #[clap(flatten)]
    global_options: GlobalOptions,

    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Runs the RPC test.
    RpcTest {
        /// Node url.
        #[clap(long, short = 'n')]
        node_url: String,

        /// Block number.
        #[clap(long, short = 'b')]
        block_number: u64,
    },

    /// Writes the RPC queries to json files.
    WriteRpcRepliesToJson {
        /// Node url.
        #[clap(long, short = 'n')]
        node_url: String,

        /// Block number.
        #[clap(long, short = 'b')]
        block_number: u64,

        // Directory path to json file. Default:
        // "./crates/blockifier_reexecution/resources/block_{block_number}/reexecution_data.json".
        // TODO(Aner): add possibility to retrieve files from gc bucket.
        #[clap(long, short = 'd', default_value = None)]
        full_file_path: Option<String>,
    },

    /// Writes the RPC queries of all (selected) blocks to json files.
    WriteAll {
        /// Node url.
        #[clap(long, short = 'n')]
        node_url: String,

        /// Block numbers. If not specified, blocks are retrieved from
        /// get_block_numbers_for_reexecution().
        #[clap(long, short = 'b', num_args = 0..)]
        block_numbers: Vec<u64>,

        // Directory path to json files directory. Default:
        // "./crates/blockifier_reexecution/resources".
        // TODO(Aner): add possibility to retrieve files from gc bucket.
        #[clap(long, short = 'd', default_value = None)]
        directory_path: Option<String>,
    },

    // Reexecutes the block from JSON files.
    ReexecuteBlock {
        /// Block number.
        #[clap(long, short = 'b')]
        block_number: u64,

        // Directory path to json files. Default:
        // "./crates/blockifier_reexecution/resources/block_{block_number}/reexecution_data.json".
        // TODO(Aner): add possibility to retrieve files from gc bucket.
        #[clap(long, short = 'd', default_value = None)]
        full_file_path: Option<String>,
    },
}

#[derive(Debug, Args)]
struct GlobalOptions {}

/// Main entry point of the blockifier reexecution CLI.
/// TODO(Aner): Add concurrency to the reexecution tests (using tokio).
fn main() {
    let args = BlockifierReexecutionCliArgs::parse();

    match args.command {
        Command::RpcTest { node_url, block_number } => {
            println!("Running RPC test for block number {block_number} using node url {node_url}.",);

            let config = RpcStateReaderConfig {
                url: node_url,
                json_rpc_version: JSON_RPC_VERSION.to_string(),
            };

            reexecute_and_verify_correctness(ConsecutiveTestStateReaders::new(
                BlockNumber(block_number - 1),
                Some(config),
                false,
            ));

            // Compare the expected and actual state differences
            // by avoiding discrepancies caused by insertion order
            println!("RPC test passed successfully.");
        }

        Command::WriteRpcRepliesToJson { node_url, block_number, full_file_path } => {
            let full_file_path = full_file_path.unwrap_or(format!(
                "./crates/blockifier_reexecution/resources/block_{block_number}/reexecution_data.\
                 json"
            ));

            write_block_reexecution_data_to_file(
                BlockNumber(block_number),
                &full_file_path,
                node_url,
            );
        }

        Command::WriteAll { node_url, block_numbers, directory_path } => {
            let directory_path =
                directory_path.unwrap_or("./crates/blockifier_reexecution/resources".to_string());

            let block_numbers = match block_numbers.len() {
                0 => get_block_numbers_for_reexecution(),
                _ => block_numbers.into_iter().map(BlockNumber).collect(),
            };

            println!("Computing reexecution data for blocks {block_numbers:?}.");

            // TODO(Aner): Execute in parallel.
            for block_number in block_numbers {
                let full_file_path =
                    format!("{directory_path}/block_{block_number}/reexecution_data.json");

                write_block_reexecution_data_to_file(
                    block_number,
                    &full_file_path,
                    node_url.clone(),
                );
            }
        }

        Command::ReexecuteBlock { block_number, full_file_path } => {
            let full_file_path = full_file_path.unwrap_or(format!(
                "./crates/blockifier_reexecution/resources/block_{block_number}/reexecution_data.\
                 json"
            ));

            reexecute_and_verify_correctness(
                OfflineConsecutiveStateReaders::new_from_file(&full_file_path).unwrap(),
            );

            println!("Reexecution test for block {block_number} passed successfully.");
        }
    }
}
