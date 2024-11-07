use cairo_lang_sierra::ProgramParser;
use cairo_native::executor::AotContractExecutor;
use cairo_native::OptLevel;
use std::path::PathBuf;
use std::process;

use clap::Parser;

#[derive(Parser, Debug)]
#[clap(version, verbatim_doc_comment)]
struct Args {
    /// The path of the Sierra file to compile.
    path: PathBuf,
    /// The output file name (default: stdout).
    output: Option<String>,
}

const DEFAULT_SIERRA_FILE: &str = "sierra_file.sierra";
const DEFAULT_OUTPUT_FILE: &str = "output_file";

fn main() {
    let args = Args::parse();
    println!("Path : {:?}", args.path);
    println!("Child PID: {}", process::id());
    let path = PathBuf::from(DEFAULT_SIERRA_FILE);

    let sierra_str = std::fs::read_to_string(&path).unwrap_or_else(|err| {
        eprintln!("Error reading Sierra file: {}", err);
        process::exit(1);
    });
    let sierra_program = ProgramParser::new().parse(&sierra_str).unwrap_or_else(|err| {
        eprintln!("Error parsing Sierra file: {}", err);
        process::exit(1);
    });

    let _contract_executor = AotContractExecutor::new(&sierra_program, OptLevel::default())
        .unwrap()
        .save(DEFAULT_OUTPUT_FILE);
}
