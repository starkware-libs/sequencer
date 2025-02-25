use clap::{Parser, Subcommand};
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::reload::Handle;
use tracing_subscriber::Registry;

use crate::os_cli::commands::parse_and_run_os;
use crate::os_cli::tests::python_tests::OsPythonTestRunner;
use crate::shared_utils::types::{run_python_test, IoArgs, PythonTestArg};

#[derive(Parser, Debug)]
pub struct OsCliCommand {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    PythonTest(PythonTestArg),
    RunOsStateless {
        #[clap(flatten)]
        io_args: IoArgs,
    },
}

pub async fn run_os_cli(
    os_command: OsCliCommand,
    _log_filter_handle: Handle<LevelFilter, Registry>,
) {
    info!("Starting starknet-os-cli with command: \n{:?}", os_command);
    match os_command.command {
        Command::PythonTest(python_test_arg) => {
            run_python_test::<OsPythonTestRunner>(python_test_arg).await;
        }
        Command::RunOsStateless { io_args: IoArgs { input_path, output_path } } => {
            parse_and_run_os(input_path, output_path);
        }
    }
}
