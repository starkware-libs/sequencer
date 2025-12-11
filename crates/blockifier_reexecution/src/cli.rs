use std::collections::HashMap;
use std::fs::read_to_string;

use clap::{Args, Parser, Subcommand};
use starknet_api::block::BlockNumber;
use starknet_api::core::{chain_id_from_hex_str, ChainId};

use crate::errors::{ReexecutionError, ReexecutionResult};

pub const FULL_RESOURCES_DIR: &str = "./crates/blockifier_reexecution/resources";

/// BlockifierReexecution CLI.
#[derive(Debug, Parser)]
#[clap(name = "blockifier-reexecution-cli", version)]
pub struct BlockifierReexecutionCliArgs {
    #[clap(flatten)]
    pub global_options: GlobalOptions,

    #[clap(subcommand)]
    pub command: Command,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum SupportedChainId {
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
#[clap(group(
    clap::ArgGroup::new("chain_id_group")
        .args(&["chain_id", "custom_chain_id"])
))]
pub struct RpcArgs {
    /// Node url.
    #[clap(long, short = 'n')]
    pub node_url: String,

    /// Optional chain ID (if not provided, it will be guessed from the node url).
    /// Supported values: mainnet, testnet, integration.
    #[clap(long, short = 'c')]
    pub chain_id: Option<SupportedChainId>,

    /// Optional custom chain ID as hex string (e.g., "0x534e5f4d41494e").
    #[clap(long)]
    pub custom_chain_id: Option<String>,
}

impl RpcArgs {
    pub fn parse_chain_id(&self) -> ChainId {
        if let Some(chain_id) = &self.chain_id {
            return chain_id.clone().into();
        }
        if let Some(hex_str) = &self.custom_chain_id {
            return chain_id_from_hex_str(hex_str).expect("Failed to parse hex chain id");
        }
        guess_chain_id_from_node_url(self.node_url.as_str()).unwrap()
    }
}

#[derive(clap::Subcommand, Debug)]
pub enum TransactionInput {
    /// Load transaction from a JSON file.
    FromFile { tx_path: String },

    /// Fetch transaction by hash from RPC.
    FromHash { tx_hash: String },
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Runs the RPC test.
    RpcTest {
        #[clap(flatten)]
        rpc_args: RpcArgs,

        /// Block number.
        #[clap(long, short = 'b')]
        block_number: u64,
    },

    /// Reexecutes a single transaction from a JSON file using RPC to fetch block context.
    ReexecuteSingleTx {
        #[clap(flatten)]
        rpc_args: RpcArgs,

        /// Block number.
        #[clap(long, short = 'b')]
        block_number: u64,

        /// Select how to provide the transaction input.
        #[clap(subcommand)]
        tx_input: TransactionInput,
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
    UploadFiles {
        /// Block numbers. If not specified, blocks are retrieved from
        /// get_block_numbers_for_reexecution().
        #[clap(long, short = 'b', num_args = 1.., default_value = None)]
        block_numbers: Option<Vec<u64>>,

        // Directory path to json files directory. Default:
        // "./crates/blockifier_reexecution/resources".
        #[clap(long, short = 'd', default_value = None)]
        directory_path: Option<String>,
    },

    // Download all (selected) blocks from the gc bucket.
    DownloadFiles {
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

#[derive(Debug, Args)]
pub struct GlobalOptions {}

pub fn parse_block_numbers_args(block_numbers: Option<Vec<u64>>) -> Vec<BlockNumber> {
    block_numbers
        .map(|block_numbers| block_numbers.into_iter().map(BlockNumber).collect())
        .unwrap_or_else(|| get_block_numbers_for_reexecution(None))
}

/// Returns the block numbers for re-execution.
/// There is a block number for each Starknet Version (starting v0.13)
/// And some additional blocks with specific transactions.
pub fn get_block_numbers_for_reexecution(relative_path: Option<String>) -> Vec<BlockNumber> {
    let file_path = relative_path.unwrap_or_default()
        + &(FULL_RESOURCES_DIR.to_string() + "/../block_numbers_for_reexecution.json");
    let block_numbers_examples: HashMap<String, u64> =
        serde_json::from_str(&read_to_string(file_path.clone()).expect(
            &("Failed to read the block_numbers_for_reexecution file at ".to_string() + &file_path),
        ))
        .expect("Failed to deserialize block header");
    block_numbers_examples.values().cloned().map(BlockNumber).collect()
}

pub fn guess_chain_id_from_node_url(node_url: &str) -> ReexecutionResult<ChainId> {
    match (
        node_url.contains("mainnet"),
        node_url.contains("sepolia"),
        node_url.contains("integration"),
    ) {
        (true, false, false) => Ok(ChainId::Mainnet),
        (false, true, false) => Ok(ChainId::Sepolia),
        // Integration URLs may contain the word "sepolia".
        (false, _, true) => Ok(ChainId::IntegrationSepolia),
        _ => Err(ReexecutionError::AmbiguousChainIdFromUrl(node_url.to_string())),
    }
}
