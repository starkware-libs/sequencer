use std::path::PathBuf;
use std::process;

use cairo_lang_sierra::program::Program;
use cairo_lang_starknet_classes::contract_class::ContractClass;
use cairo_native::executor::AotContractExecutor;
use cairo_native::OptLevel;
use clap::Parser;

#[derive(Parser, Debug)]
#[clap(version, verbatim_doc_comment)]
struct Args {
    /// The path of the Sierra file to compile.
    path: PathBuf,
    /// The output file path.
    output: PathBuf,
}

pub(crate) fn main() {
    let args = Args::parse();
    let path = args.path;
    let output = args.output;

    let (contract_class, sierra_program) = load_sierra_program_from_file(&path);

    // TODO(Avi, 01/12/2024): Test different optimization levels for best performance.
    let mut contract_executor = AotContractExecutor::new(
        &sierra_program,
        &contract_class.entry_points_by_type,
        OptLevel::default(),
    )
    .unwrap_or_else(|err| {
        eprintln!("Error compiling Sierra program: {}", err);
        process::exit(1);
    });
    contract_executor.save(output.clone()).unwrap_or_else(|err| {
        eprintln!("Error saving compiled program: {}", err);
        process::exit(1);
    });
}

fn load_sierra_program_from_file(path: &PathBuf) -> (ContractClass, Program) {
    let raw_contract_class = std::fs::read_to_string(path).unwrap_or_else(|err| {
        eprintln!("Error reading Sierra file: {}", err);
        process::exit(1);
    });
    let contract_class: ContractClass =
        serde_json::from_str(&raw_contract_class).unwrap_or_else(|err| {
            eprintln!("Error deserializing Sierra file into contract class: {}", err);
            process::exit(1);
        });
    (
        contract_class.clone(),
        contract_class.extract_sierra_program().unwrap_or_else(|err| {
            eprintln!("Error extracting Sierra program from contract class: {}", err);
            process::exit(1);
        }),
    )
}
