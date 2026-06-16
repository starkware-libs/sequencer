use clap::{Args, Parser, Subcommand};
use starknet_api::block::{BlockNumber, StarknetVersion};
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

    /// Continuously reexecute blocks via RPC and verify state diff correctness.
    RpcReplay {
        #[clap(flatten)]
        rpc_args: RpcArgs,

        /// First block to reexecute.
        #[clap(long)]
        start_block: u64,

        /// Last block to reexecute (inclusive). If omitted, runs forever.
        #[clap(long)]
        end_block: Option<u64>,

        /// Number of parallel worker threads.
        #[clap(long, default_value = "1")]
        n_workers: usize,

        /// Run each block twice (native and CASM) and compare the resulting state diffs.
        /// Overrides `min_sierra_version_for_sierra_gas` to `0.0.0` so all contracts use sierra
        /// gas, ensuring a fair comparison. Requires the `cairo_native` feature.
        #[clap(long)]
        compare_native: bool,

        /// Prefetch initial reads before execution with starknet_simulateTransactions.
        #[clap(long, default_value = "true", action = clap::ArgAction::Set)]
        prefetch_initial_reads: bool,
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
        .unwrap_or_else(get_block_numbers_for_reexecution)
}

/// A mainnet block re-executed for each supported Starknet version.
/// Multiple blocks may map to the same version; that is allowed. Every version from
/// [`StarknetVersion::V0_13_0`] up to (but not including) the latest two must appear here -
/// enforced by `test_all_starknet_versions_are_reexecuted`. The latest two versions are exempt
/// because a freshly released version may not have a mainnet block to re-execute yet.
pub static REEXECUTION_BLOCK_PER_VERSION: &[(StarknetVersion, BlockNumber)] = &[
    (StarknetVersion::V0_13_0, BlockNumber(600001)),
    (StarknetVersion::V0_13_1, BlockNumber(620978)),
    (StarknetVersion::V0_13_1_1, BlockNumber(649367)),
    (StarknetVersion::V0_13_2, BlockNumber(685878)),
    (StarknetVersion::V0_13_2_1, BlockNumber(700000)),
    (StarknetVersion::V0_13_3, BlockNumber(1000000)),
    (StarknetVersion::V0_13_4, BlockNumber(1257000)),
    (StarknetVersion::V0_13_5, BlockNumber(1300000)),
    (StarknetVersion::V0_13_6, BlockNumber(1743490)),
    (StarknetVersion::V0_14_0, BlockNumber(2509604)),
    (StarknetVersion::V0_14_1, BlockNumber(4448394)),
    // A second 0.14.1 block, covering the sierra-gas revert path.
    (StarknetVersion::V0_14_1, BlockNumber(6481044)),
    (StarknetVersion::V0_14_2, BlockNumber(9023035)),
];

/// Additional blocks exercising specific transaction types or RPC versions. These are extra
/// coverage and are not tied to Starknet-version coverage; the label documents what each exercises.
pub static REEXECUTION_EXAMPLE_BLOCKS: &[(&str, BlockNumber)] = &[
    ("first_0.13.5_rpc_v0_8", BlockNumber(1400000)),
    ("second_0.13.5_rpc_v0_8", BlockNumber(1450000)),
    ("invoke_with_replace_class_syscall", BlockNumber(780008)),
    ("invoke_with_deploy_syscall", BlockNumber(870136)),
    ("deploy_account_v1", BlockNumber(837408)),
    ("deploy_account_v3", BlockNumber(837792)),
    ("declare_v1", BlockNumber(837461)),
    ("declare_v2", BlockNumber(822636)),
    ("declare_v3", BlockNumber(825013)),
    ("l1_handler", BlockNumber(868429)),
];

/// Returns all block numbers for re-execution: one (or more) per Starknet version (starting v0.13)
/// plus some additional blocks with specific transactions.
pub fn get_block_numbers_for_reexecution() -> Vec<BlockNumber> {
    REEXECUTION_BLOCK_PER_VERSION
        .iter()
        .map(|(_version, block_number)| *block_number)
        .chain(REEXECUTION_EXAMPLE_BLOCKS.iter().map(|(_label, block_number)| *block_number))
        .collect()
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
