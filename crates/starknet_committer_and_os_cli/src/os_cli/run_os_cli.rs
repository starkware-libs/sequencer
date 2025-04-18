use clap::{Parser, Subcommand};
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::reload::Handle;
use tracing_subscriber::Registry;

use crate::os_cli::commands::{
    compile_and_dump_module,
    dump_aggregator_program,
    dump_os_program,
    dump_program_hash,
    parse_and_run_os,
};
use crate::os_cli::tests::python_tests::OsPythonTestRunner;
use crate::shared_utils::types::{run_python_test, IoArgs, PythonTestArg};

#[derive(Parser, Debug)]
pub struct OsCliCommand {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    CompileAndDumpModule {
        /// Path to module in the OS program crate.
        #[clap(long, short = 'm')]
        path_to_module: String,

        /// File path to output.
        #[clap(long, short = 'o', default_value = "stdout")]
        output_path: String,
    },
    DumpAggregatorProgram {
        /// File path to output.
        #[clap(long, short = 'o', default_value = "stdout")]
        output_path: String,
    },
    DumpOsProgram {
        /// File path to output.
        #[clap(long, short = 'o', default_value = "stdout")]
        output_path: String,
    },
    DumpProgramHash {
        /// File path to output.
        #[clap(long, short = 'o', default_value = "stdout")]
        output_path: String,
    },
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
        Command::CompileAndDumpModule { path_to_module, output_path } => {
            compile_and_dump_module(path_to_module, output_path)
        }
        Command::DumpAggregatorProgram { output_path } => dump_aggregator_program(output_path),
        Command::DumpOsProgram { output_path } => dump_os_program(output_path),
        Command::DumpProgramHash { output_path } => dump_program_hash(output_path),
        Command::PythonTest(python_test_arg) => {
            run_python_test::<OsPythonTestRunner>(python_test_arg).await;
        }
        Command::RunOsStateless { io_args: IoArgs { input_path, output_path } } => {
            parse_and_run_os(input_path, output_path);
        }
    }
}
