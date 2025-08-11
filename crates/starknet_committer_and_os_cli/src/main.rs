use clap::{Args, Parser, Subcommand};
use starknet_committer_and_os_cli::block_hash_cli::run_block_hash_cli::{
    run_block_hash_cli,
    BlockHashCliCommand,
};
use starknet_committer_and_os_cli::committer_cli::run_committer_cli::{
    run_committer_cli,
    CommitterCliCommand,
};
use starknet_committer_and_os_cli::os_cli::run_os_cli::{run_os_cli, OsCliCommand};
use starknet_committer_and_os_cli::tracing_utils::configure_tracing;
use tracing::info;

/// Committer and OS CLI.
#[derive(Debug, Parser)]
#[clap(name = "committer-and-os-cli", version)]
struct CliArgs {
    #[clap(flatten)]
    global_options: GlobalOptions,

    #[clap(subcommand)]
    command: CommitterOrOsCommand,
}

#[derive(Debug, Subcommand)]
enum CommitterOrOsCommand {
    /// Run Committer CLI.
    Committer(CommitterCliCommand),
    /// Run BlockHash CLI.
    BlockHash(BlockHashCliCommand),
    /// Run OS CLI.
    OS(OsCliCommand),
}

#[derive(Debug, Args)]
struct GlobalOptions {}

#[tokio::main]
/// Main entry point of the committer & OS CLI.
async fn main() {
    // Initialize the logger. The log_filter_handle is used to change the log level. The
    // default log level is INFO.
    let log_filter_handle = configure_tracing();

    let args = CliArgs::parse();
    println!("Parsed CLI args: {:?}", args);
    println!("$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$");
    info!("Starting committer & OS cli with args: \n{:?}", args);

    match args.command {
        CommitterOrOsCommand::Committer(command) => {
            run_committer_cli(command, log_filter_handle).await;
        }
        CommitterOrOsCommand::OS(command) => {
            run_os_cli(command, log_filter_handle).await;
        }
        CommitterOrOsCommand::BlockHash(command) => {
            run_block_hash_cli(command).await;
        }
    }
}
