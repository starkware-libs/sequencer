use clap::{Args, Parser, Subcommand};
use starknet_committer_and_os_cli::committer_cli::run_committer_cli::{
    CommitterCLICommand, run_committer_cli,
};
use starknet_committer_and_os_cli::os_cli::run_os_cli::{OSCLICommand, run_os_cli};
use starknet_committer_and_os_cli::tracing_utils::configure_tracing;
use tracing::info;

/// Committer and OS CLI.
#[derive(Debug, Parser)]
#[clap(name = "committer-and-os-cli", version)]
pub struct CliArgs {
    #[clap(flatten)]
    global_options: GlobalOptions,

    #[clap(subcommand)]
    command: CommitterOrOSCommand,
}

#[derive(Debug, Subcommand)]
enum CommitterOrOSCommand {
    Committer(CommitterCLICommand),
    OS(OSCLICommand),
}

#[derive(Debug, Args)]
struct GlobalOptions {}

#[tokio::main]
/// Main entry point of the committe & OS CLI.
async fn main() {
    // Initialize the logger. The log_filter_handle is used to change the log level. The
    // default log level is INFO.
    let log_filter_handle = configure_tracing();

    let args = CliArgs::parse();
    info!("Starting committer & OS cli with args: \n{:?}", args);

    match args.command {
        CommitterOrOSCommand::Committer(command) => {
            run_committer_cli(command, log_filter_handle).await;
        }
        CommitterOrOSCommand::OS(command) => {
            run_os_cli(command, log_filter_handle).await;
        }
    }
}
