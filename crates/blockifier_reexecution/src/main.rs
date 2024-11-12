use blockifier_reexecution::state_reader::test_state_reader::{
    ConsecutiveTestStateReaders,
    OfflineConsecutiveStateReaders,
};
use blockifier_reexecution::state_reader::utils::{
    get_block_numbers_for_reexecution,
    guess_chain_id_from_node_url,
    reexecute_and_verify_correctness,
    write_block_reexecution_data_to_file,
    JSON_RPC_VERSION,
};
use clap::{Args, Parser, Subcommand};
use starknet_api::block::BlockNumber;
use starknet_api::core::ChainId;
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

#[derive(clap::ValueEnum, Clone, Debug)]
enum SupportedChainId {
    Mainnet,
    Testnet,
    Integration,
}

impl From<SupportedChainId> for ChainId {
    fn from(chain_id: SupportedChainId) -> Self {
        match chain_id {
            SupportedChainId::Mainnet => Self::Mainnet,
            SupportedChainId::Testnet => Self::Sepolia,
            SupportedChainId::Integration => Self::IntegrationSepolia,
        }
    }
}

#[derive(Debug, Args)]
struct RpcArgs {
    /// Node url.
    #[clap(long, short = 'n')]
    node_url: String,

    /// Optional chain ID (if not provided, it will be guessed from the node url).
    #[clap(long, short = 'c')]
    chain_id: Option<SupportedChainId>,
}

impl RpcArgs {
    pub(crate) fn parse_chain_id(&self) -> ChainId {
        self.chain_id
            .clone()
            .map(ChainId::from)
            .unwrap_or(guess_chain_id_from_node_url(self.node_url.as_str()).unwrap())
    }
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Runs the RPC test.
    RpcTest {
        #[clap(flatten)]
        rpc_args: RpcArgs,

        /// Block number.
        #[clap(long, short = 'b')]
        block_number: u64,
    },

    /// Writes the RPC queries to json files.
    WriteRpcRepliesToJson {
        #[clap(flatten)]
        rpc_args: RpcArgs,

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
        #[clap(flatten)]
        rpc_args: RpcArgs,

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
        Command::RpcTest { block_number, rpc_args } => {
            println!(
                "Running RPC test for block number {block_number} using node url {}.",
                rpc_args.node_url
            );

            let config = RpcStateReaderConfig {
                url: rpc_args.node_url.clone(),
                json_rpc_version: JSON_RPC_VERSION.to_string(),
            };

            reexecute_and_verify_correctness(ConsecutiveTestStateReaders::new(
                BlockNumber(block_number - 1),
                Some(config),
                rpc_args.parse_chain_id(),
                false,
            ));

            // Compare the expected and actual state differences
            // by avoiding discrepancies caused by insertion order
            println!("RPC test passed successfully.");
        }

        Command::WriteRpcRepliesToJson { block_number, full_file_path, rpc_args } => {
            let full_file_path = full_file_path.unwrap_or(format!(
                "./crates/blockifier_reexecution/resources/block_{block_number}/reexecution_data.\
                 json"
            ));

            write_block_reexecution_data_to_file(
                BlockNumber(block_number),
                &full_file_path,
                rpc_args.node_url.clone(),
                rpc_args.parse_chain_id(),
            );
        }

        Command::WriteAll { block_numbers, directory_path, rpc_args } => {
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
                    rpc_args.node_url.clone(),
                    rpc_args.parse_chain_id(),
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
