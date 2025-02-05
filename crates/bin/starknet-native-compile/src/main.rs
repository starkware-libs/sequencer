use std::path::PathBuf;
use std::process;

use cairo_native::executor::AotContractExecutor;
use cairo_native::OptLevel;
use clap::Parser;

use crate::utils::load_sierra_program_from_file;
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

    let (contract_class, sierra_program, sierra_version) = load_sierra_program_from_file(&path);

    // TODO(Avi, 01/12/2024): Test different optimization levels for best performance.
    AotContractExecutor::new_into(
        &sierra_program,
        &contract_class.entry_points_by_type,
        sierra_version.clone(),
        output.clone(),
        OptLevel::default(),
    )
    .unwrap_or_else(|err| {
        eprintln!("Error compiling Sierra program: {}", err);
        process::exit(1);
    })
    .unwrap_or_else(|| {
        eprintln!("Failed to take lock on path {} ", output.display());
        process::exit(1);
    });
}
