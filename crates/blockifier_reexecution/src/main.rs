use blockifier_reexecution::assert_eq_state_diff;
use blockifier_reexecution::state_reader::reexecution_state_reader::ReexecutionStateReader;
use blockifier_reexecution::state_reader::test_state_reader::{
    ConsecutiveStateReaders,
    ConsecutiveTestStateReaders,
    OfflineConsecutiveStateReaders,
    SerializableOfflineReexecutionData,
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

    // Reexecutes the block from JSON files.
    ReexecuteBlock {
        /// Block number.
        #[clap(long, short = 'b')]
        block_number: u64,

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

            // Compare the expected and actual state differences
            // by avoiding discrepancies caused by insertion order
            assert_eq_state_diff!(expected_state_diff, actual_state_diff);
            println!("RPC test passed successfully.");
        }

        Command::WriteRpcRepliesToJson {
            url_and_block_number: SharedArgs { node_url, block_number },
            directory_path,
        } => {
            let directory_path = directory_path.unwrap_or(format!(
                "./crates/blockifier_reexecution/resources/block_{block_number}/"
            ));

            // TODO(Aner): refactor to reduce code duplication.
            let config = RpcStateReaderConfig {
                url: node_url,
                json_rpc_version: JSON_RPC_VERSION.to_string(),
            };

            let ConsecutiveTestStateReaders { last_block_state_reader, next_block_state_reader } =
                ConsecutiveTestStateReaders::new(BlockNumber(block_number - 1), Some(config), true);

            let block_info_next_block = next_block_state_reader.get_block_info().unwrap();

            let starknet_version = next_block_state_reader.get_starknet_version().unwrap();

            let state_diff_next_block = next_block_state_reader.get_state_diff().unwrap();

            let transactions_next_block = next_block_state_reader.get_all_txs_in_block().unwrap();

            let blockifier_transactions_next_block = &last_block_state_reader
                .api_txs_to_blockifier_txs_next_block(transactions_next_block.clone())
                .unwrap();

            let mut transaction_executor = last_block_state_reader
                .get_transaction_executor(
                    next_block_state_reader.get_block_context().unwrap(),
                    None,
                )
                .unwrap();

            transaction_executor.execute_txs(blockifier_transactions_next_block);

            let block_state = transaction_executor.block_state.unwrap();
            let initial_reads = block_state.get_initial_reads().unwrap();

            let contract_class_mapping =
                block_state.state.get_contract_class_mapping_dumper().unwrap();

            let serializable_offline_reexecution_data = SerializableOfflineReexecutionData {
                state_maps: initial_reads.into(),
                block_info_next_block,
                starknet_version,
                transactions_next_block,
                contract_class_mapping,
                state_diff_next_block,
            };

            serializable_offline_reexecution_data
                .write_to_file(&directory_path, "reexecution_data.json")
                .unwrap();

            println!(
                "RPC replies required for reexecuting block {block_number} written to json file."
            );
        }

        Command::ReexecuteBlock { block_number, directory_path } => {
            let full_file_path = directory_path.unwrap_or(format!(
                "./crates/blockifier_reexecution/resources/block_{block_number}"
            )) + "/reexecution_data.json";

            let serializable_offline_reexecution_data =
                SerializableOfflineReexecutionData::read_from_file(&full_file_path).unwrap();

            let reexecution_state_readers =
                OfflineConsecutiveStateReaders::new(serializable_offline_reexecution_data.into());

            let mut expected_state_diff =
                reexecution_state_readers.get_next_block_state_diff().unwrap();

            let all_txs_in_next_block = reexecution_state_readers.get_next_block_txs().unwrap();

            let mut transaction_executor =
                reexecution_state_readers.get_transaction_executor(None).unwrap();

            transaction_executor.execute_txs(&all_txs_in_next_block);
            // Finalize block and read actual statediff.
            let (actual_state_diff, _, _) =
                transaction_executor.finalize().expect("Couldn't finalize block");

            // TODO(Aner): compute correct block hash at storage slot 0x1 instead of removing it.
            expected_state_diff.storage_updates.shift_remove(&ContractAddress(1_u128.into()));
            assert_eq!(expected_state_diff, actual_state_diff);

            println!("Reexecution test for block {block_number} passed successfully.");
        }
    }
}
