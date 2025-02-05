use std::path::PathBuf;
use std::process;

use cairo_native::executor::AotContractExecutor;
use cairo_native::OptLevel;
use clap::Parser;

use crate::utils::{get_sierra_verizon_from_program, load_sierra_program_from_file};
const N_RETRIES: usize = 10;

mod utils;

#[derive(Parser, Debug)]
#[clap(version, verbatim_doc_comment)]
struct Args {
    /// The path of the Sierra file to compile.
    path: PathBuf,
    /// The output file path.
    output: PathBuf,
}

fn main() {
    // TODO(Avi, 01/12/2024): Find a way to restrict time, memory and file size during compilation.
    let args = Args::parse();
    let path = args.path;
    let output = args.output;

    let (contract_class, sierra_program) = load_sierra_program_from_file(&path);
    let raw_sierra_program = contract_class
        .sierra_program
        .clone()
        .into_iter()
        .map(|felt| felt.value)
        .collect::<Vec<_>>();
    let sierra_version = get_sierra_verizon_from_program(&raw_sierra_program);

    // TODO(Avi, 01/12/2024): Test different optimization levels for best performance.
    let mut contract_executor = AotContractExecutor::new_into(
        &sierra_program,
        &contract_class.entry_points_by_type,
        sierra_version.clone(),
        output.clone(),
        OptLevel::default(),
    )
    .unwrap_or_else(|err| {
        eprintln!("Error compiling Sierra program: {}", err);
        process::exit(1);
    });
    for _ in 0..N_RETRIES {
        if contract_executor.is_some() {
            break;
        }
        eprintln!("Failed to take lock on path {} . Retrying...", output.display());
        contract_executor = AotContractExecutor::from_path(&output).unwrap_or_else(|err| {
            eprintln!("Error deserializing Sierra file into contract class: {}.", err);
            process::exit(1);
        });
    }
}
