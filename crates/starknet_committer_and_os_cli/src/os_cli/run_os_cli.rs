use clap::{Parser, Subcommand};
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::reload::Handle;
use tracing_subscriber::Registry;

#[derive(Parser, Debug)]
pub struct OSCLICommand {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    NotImplemented,
}

pub async fn run_os_cli(
    os_command: OSCLICommand,
    _log_filter_handle: Handle<LevelFilter, Registry>,
) {
    match os_command.command {
        Command::NotImplemented => {
            info!("Not implemented");
        }
    }
}
