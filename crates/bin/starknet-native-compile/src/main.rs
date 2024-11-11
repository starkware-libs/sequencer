use crate::utils::load_sierra_program_from_file;
use cairo_native::executor::AotContractExecutor;
use cairo_native::OptLevel;
use std::path::PathBuf;
use std::process;

use clap::Parser;

mod utils;

#[derive(Parser, Debug)]
#[clap(version, verbatim_doc_comment)]
struct Args {
    /// The path of the Sierra file to compile.
    path: PathBuf,
    /// The output file name (default: output.so).
    output: Option<String>,
}

const DEFAULT_OUTPUT_FILE: &str = "./output.so";

fn main() {
    // TODO(Avi, 01/12/2024): Find a way to restrict time, memory and file size during compilation.
    let args = Args::parse();
    let path = args.path;
    let output = args.output.unwrap_or_else(|| DEFAULT_OUTPUT_FILE.to_string());

    println!("Loading Sierra program from file: {:?}", &path);
    let sierra_program = load_sierra_program_from_file(&path);

    println!("Compiling Sierra program into file {:?}", &output);
    let start = std::time::Instant::now();
    let mut contract_executor = AotContractExecutor::new(&sierra_program, OptLevel::default())
        .unwrap_or_else(|err| {
            eprintln!("Error compiling Sierra program: {}", err);
            process::exit(1);
        });
    contract_executor.save(output.clone()).unwrap_or_else(|err| {
        eprintln!("Error saving compiled program: {}", err);
        process::exit(1);
    });
    println!("Compilation successful. Elapsed: {:?}", start.elapsed());
}
