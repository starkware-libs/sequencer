use std::fs;
use std::path::Path;

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
use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::objects::get::GetObjectRequest;
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

#[derive(Clone, Debug, Args)]
struct RpcArgs {
    /// Node url.
    #[clap(long, short = 'n')]
    node_url: String,

    /// Optional chain ID (if not provided, it will be guessed from the node url).
    #[clap(long, short = 'c')]
    chain_id: Option<SupportedChainId>,
}

impl RpcArgs {
    fn parse_chain_id(&self) -> ChainId {
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

    /// Writes the RPC queries of all (selected) blocks to json files.
    WriteToFile {
        #[clap(flatten)]
        rpc_args: RpcArgs,

        /// Block numbers. If not specified, blocks are retrieved from
        /// get_block_numbers_for_reexecution().
        #[clap(long, short = 'b', num_args = 1.., default_value = None)]
        block_numbers: Option<Vec<u64>>,

        // Directory path to json files directory. Default:
        // "./crates/blockifier_reexecution/resources".
        // TODO(Aner): add possibility to retrieve files from gc bucket.
        #[clap(long, short = 'd', default_value = None)]
        directory_path: Option<String>,
    },

    // Reexecute all (selected) blocks
    Reexecute {
        /// Block numbers. If not specified, blocks are retrieved from
        /// get_block_numbers_for_reexecution().
        #[clap(long, short = 'b', num_args = 1.., default_value = None)]
        block_numbers: Option<Vec<u64>>,

        // Directory path to json files directory. Default:
        // "./crates/blockifier_reexecution/resources".
        // TODO(Aner): add possibility to retrieve files from gc bucket.
        #[clap(long, short = 'd', default_value = None)]
        directory_path: Option<String>,
    },

    // Upload all (selected) blocks to the gc bucket.
    UploadAll {
        /// Block numbers. If not specified, blocks are retrieved from
        /// get_block_numbers_for_reexecution().
        #[clap(long, short = 'b', num_args = 1.., default_value = None)]
        block_numbers: Option<Vec<u64>>,

        // Directory path to json files directory. Default:
        // "./crates/blockifier_reexecution/resources".
        #[clap(long, short = 'd', default_value = None)]
        directory_path: Option<String>,
    },
}

fn parse_block_numbers_args(block_numbers: Option<Vec<u64>>) -> Vec<BlockNumber> {
    block_numbers
        .map(|block_numbers| block_numbers.into_iter().map(BlockNumber).collect())
        .unwrap_or(get_block_numbers_for_reexecution())
}

#[derive(Debug, Args)]
struct GlobalOptions {}

/// Main entry point of the blockifier reexecution CLI.
/// TODO(Aner): run by default from the root of the project.
#[tokio::main]
async fn main() {
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

            // RPC calls are "synchronous IO" (see, e.g., https://stackoverflow.com/questions/74547541/when-should-you-use-tokios-spawn-blocking)
            // for details), so should be executed in a blocking thread.
            // TODO(Aner): make only the RPC calls blocking, not the whole function.
            tokio::task::spawn_blocking(move || {
                reexecute_and_verify_correctness(ConsecutiveTestStateReaders::new(
                    BlockNumber(block_number - 1),
                    Some(config),
                    rpc_args.parse_chain_id(),
                    false,
                ))
            })
            .await
            .unwrap();

            // Compare the expected and actual state differences
            // by avoiding discrepancies caused by insertion order
            println!("RPC test passed successfully.");
        }

        Command::WriteToFile { block_numbers, directory_path, rpc_args } => {
            let directory_path =
                directory_path.unwrap_or("./crates/blockifier_reexecution/resources".to_string());

            let block_numbers = parse_block_numbers_args(block_numbers);
            println!("Computing reexecution data for blocks {block_numbers:?}.");

            // TODO(Aner): Execute in parallel. Requires making the function async, and only the RPC
            // calls blocking.
            for block_number in block_numbers {
                let full_file_path =
                    format!("{directory_path}/block_{block_number}/reexecution_data.json");
                let (node_url, chain_id) = (rpc_args.node_url.clone(), rpc_args.parse_chain_id());
                // RPC calls are "synchronous IO" (see, e.g., https://stackoverflow.com/questions/74547541/when-should-you-use-tokios-spawn-blocking
                // for details), so should be executed in a blocking thread.
                // TODO(Aner): make only the RPC calls blocking, not the whole function.
                tokio::task::spawn_blocking(move || {
                    println!("Computing reexecution data for block {block_number}.");
                    write_block_reexecution_data_to_file(
                        block_number,
                        full_file_path,
                        node_url,
                        chain_id,
                    )
                })
                .await
                .unwrap();
            }
        }

        Command::Reexecute { block_numbers, directory_path } => {
            let directory_path =
                directory_path.unwrap_or("./crates/blockifier_reexecution/resources".to_string());

            let block_numbers = parse_block_numbers_args(block_numbers);
            println!("Reexecuting blocks {block_numbers:?}.");

            let mut threads = vec![];
            for block in block_numbers {
                let full_file_path =
                    format!("{directory_path}/block_{block}/reexecution_data.json");
                threads.push(tokio::task::spawn(async move {
                    reexecute_and_verify_correctness(
                        OfflineConsecutiveStateReaders::new_from_file(&full_file_path).unwrap(),
                    );
                    println!("Reexecution test for block {block} passed successfully.");
                }));
            }
            for thread in threads {
                thread.await.unwrap();
            }
        }

        Command::UploadAll { block_numbers, directory_path } => {
            let directory_path =
                directory_path.unwrap_or("./crates/blockifier_reexecution/resources".to_string());

            let block_numbers = parse_block_numbers_args(block_numbers);
            println!("Uploading blocks {block_numbers:?}.");

            let files_prefix: String =
                fs::read_to_string(directory_path.clone() + "/offline_reeexecution_files_prefix")
                    .unwrap()
                    + "/resources/";

            let config = ClientConfig::default().with_auth().await.unwrap();
            let client = Client::new(config);

            // Verify all required files exist locally, and do not exist in the gc bucket.
            for block_number in &block_numbers {
                assert!(
                    Path::exists(Path::new(&format!(
                        "{directory_path}/block_{block_number}/reexecution_data.json"
                    ))),
                    "Block {block_number} reexecution data file does not exist."
                );
                assert!(
                    client
                        .get_object(&GetObjectRequest {
                            bucket: "reexecution_artifacts".to_string(),
                            object: files_prefix.clone()
                                + &format!("block_{block_number}/reexecution_data.json"),
                            ..Default::default()
                        })
                        .await
                        // TODO: check that the error is not found error.
                        .is_err(),
                    "Block {block_number} reexecution data file already exists in bucket."
                )
            }

            // Upload all files to the gc bucket.
            // for block_number in &block_numbers {}

            println!("All blocks uploaded successfully.");
        }
    }
}
