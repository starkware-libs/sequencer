use std::collections::HashSet;

use clap::{Parser, Subcommand};
use serde::Serialize;
use starknet_os::hints::enum_definition::AllHints;
use starknet_types_core::felt::Felt;
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::reload::Handle;
use tracing_subscriber::Registry;

use crate::os_cli::commands::{
    dump_program,
    dump_program_hashes,
    dump_source_files,
    parse_and_run_aggregator,
    parse_and_run_os,
};
use crate::os_cli::tests::python_tests::OsPythonTestRunner;
use crate::shared_utils::types::{run_python_test, IoArgs, PythonTestArg};

#[derive(Parser, Debug)]
pub struct OsCliCommand {
    #[clap(subcommand)]
    command: Command,
}

#[derive(clap::ValueEnum, Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProgramToDump {
    Aggregator,
    AliasesTest,
    Os,
}

#[derive(Debug, Subcommand)]
enum Command {
    DumpProgram {
        /// File path to output.
        #[clap(long, short = 'o', default_value = "stdout")]
        output_path: String,

        /// Program to dump.
        #[clap(long, value_enum)]
        program: ProgramToDump,
    },
    DumpProgramHashes {
        /// File path to output.
        #[clap(long, short = 'o', default_value = "stdout")]
        output_path: String,
    },
    DumpSourceFiles {
        /// File path to output.
        #[clap(long, short = 'o')]
        output_path: String,
    },
    PythonTest(PythonTestArg),
    RunOsStateless {
        #[clap(flatten)]
        io_args: IoArgs,
    },
    RunAggregator {
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
        Command::DumpProgram { output_path, program } => dump_program(output_path, program),
        Command::DumpProgramHashes { output_path } => dump_program_hashes(output_path),
        Command::DumpSourceFiles { output_path } => dump_source_files(output_path),
        Command::PythonTest(python_test_arg) => {
            run_python_test::<OsPythonTestRunner>(python_test_arg).await;
        }
        Command::RunOsStateless { io_args: IoArgs { input_path, output_path } } => {
            parse_and_run_os(input_path, output_path);
        }
        Command::RunAggregator { io_args: IoArgs { input_path, output_path } } => {
            parse_and_run_aggregator(input_path, output_path);
        }
    }
}

#[derive(Serialize)]
pub(crate) struct OsCliOutput {
    pub(crate) os_output: Vec<Felt>,
    pub unused_hints: HashSet<AllHints>,
}

#[derive(Serialize)]
pub(crate) struct AggregatorCliOutput {
    pub(crate) aggregator_output: Vec<Felt>,
    pub unused_hints: HashSet<AllHints>,
}
