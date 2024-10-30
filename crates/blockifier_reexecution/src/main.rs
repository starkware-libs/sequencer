use blockifier_reexecution::state_reader::test_state_reader::{
    ConsecutiveStateReaders,
    ConsecutiveTestStateReaders,
};
use blockifier_reexecution::state_reader::utils::JSON_RPC_VERSION;
use clap::{Args, Parser, Subcommand};
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
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

#[derive(Args, Debug)]
struct SharedArgs {
    /// Node url.
    /// Default: https://free-rpc.nethermind.io/mainnet-juno/. Won't work for big tests.
    #[clap(long, short = 'n', default_value = "https://free-rpc.nethermind.io/mainnet-juno/")]
    node_url: String,

    /// Block number.
    #[clap(long, short = 'b')]
    block_number: u64,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Runs the RPC test.
    RpcTest {
        #[clap(flatten)]
        url_and_block_number: SharedArgs,
    },

    /// Writes the RPC queries to json files.
    WriteRpcRepliesToJson {
        #[clap(flatten)]
        url_and_block_number: SharedArgs,

        /// Directory path to json files.
        /// Default: "./crates/blockifier_reexecution/resources/block_{block_number}".
        #[clap(long, default_value = None)]
        directory_path: Option<String>,
    },
}

#[derive(Debug, Args)]
struct GlobalOptions {}

/// Main entry point of the blockifier reexecution CLI.
fn main() {
    let args = BlockifierReexecutionCliArgs::parse();

    match args.command {
        Command::RpcTest { url_and_block_number: SharedArgs { node_url, block_number } } => {
            println!("Running RPC test for block number {block_number} using node url {node_url}.",);

            let config = RpcStateReaderConfig {
                url: node_url,
                json_rpc_version: JSON_RPC_VERSION.to_string(),
            };

            let test_state_readers_last_and_current_block = ConsecutiveTestStateReaders::new(
                BlockNumber(block_number - 1),
                Some(config),
                false,
            );

            let all_txs_in_next_block =
                test_state_readers_last_and_current_block.get_next_block_txs().unwrap();

            let mut expected_state_diff =
                test_state_readers_last_and_current_block.get_next_block_state_diff().unwrap();

            let mut transaction_executor =
                test_state_readers_last_and_current_block.get_transaction_executor(None).unwrap();

            transaction_executor.execute_txs(&all_txs_in_next_block);
            // Finalize block and read actual statediff.
            let (actual_state_diff, _, _) =
                transaction_executor.finalize().expect("Couldn't finalize block");
            // TODO(Aner): compute correct block hash at storage slot 0x1 instead of removing it.
            expected_state_diff.storage_updates.shift_remove(&ContractAddress(1_u128.into()));

            if expected_state_diff != actual_state_diff {
                let expected_json = serde_json::to_value(&expected_state_diff)
                    .expect("Failed to serialize expected_state_diff");
                let actual_json = serde_json::to_value(&actual_state_diff)
                    .expect("Failed to serialize actual_state_diff");
                eprintln!("Test failed! Differences in state_diff");
                eprintln!("Expected: {expected_json}");
                eprintln!("Actual: {actual_json}");
            } else {
                println!("RPC test passed successfully.");
            }
        }
        Command::WriteRpcRepliesToJson { .. } => todo!(),
    }
}
