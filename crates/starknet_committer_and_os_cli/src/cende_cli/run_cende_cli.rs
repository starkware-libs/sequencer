use clap::{Parser, Subcommand};
use tracing::info;

use crate::cende_cli::tests::python_tests::CendePythonTestRunner;
use crate::shared_utils::types::{run_python_test, PythonTestArg};

#[derive(Parser, Debug)]
pub struct CendeCliCommand {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    PythonTest(PythonTestArg),
}

pub async fn run_cende_cli(cende_cli_command: CendeCliCommand) {
    info!("Starting cende-cli with command: \n{:?}", cende_cli_command);
    match cende_cli_command.command {
        Command::PythonTest(python_test_arg) => {
            run_python_test::<CendePythonTestRunner>(python_test_arg).await;
        }
    }
}
