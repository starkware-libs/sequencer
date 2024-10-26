use clap::{Args, Parser, Subcommand};

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
        /// Default: https://free-rpc.nethermind.io/mainnet-juno/. Won't work for big tests.
        #[clap(long, short = 'n', default_value = "https://free-rpc.nethermind.io/mainnet-juno/")]
        node_url: String,

        /// JSON RPC version, Default: 2.0.
        #[clap(long, default_value = "2.0")]
        json_rpc_version: String,

        /// Block number.
        #[clap(long, short = 'b', default_value = "70000")]
        block_number: u64,
    },
}

#[derive(Debug, Args)]
struct GlobalOptions {}

/// Main entry point of the blockifier reexecution CLI.
fn main() {
    let args = BlockifierReexecutionCliArgs::parse();

    match args.command {
        Command::RpcTest { .. } => todo!(), // TODO(Aner): Move the RPC test logic here.
    }
}
