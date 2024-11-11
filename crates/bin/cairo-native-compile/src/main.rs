use cairo_lang_sierra::program::Program;
use cairo_lang_starknet_classes::contract_class::ContractClass;
use cairo_native::executor::AotContractExecutor;
use cairo_native::OptLevel;
use std::path::{Path, PathBuf};
use std::process;

use clap::Parser;

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

    println!("Attempting to load compiled program from file {:?}", &output);
    let loaded_executor =
        AotContractExecutor::load(Path::new(DEFAULT_OUTPUT_FILE)).unwrap_or_else(|err| {
            eprintln!("Error loading compiled program: {}", err);
            process::exit(1);
        });
    println!("Program loaded successfully");
    println!("Program: {:?}", loaded_executor);
}

fn load_sierra_program_from_file(path: &PathBuf) -> Program {
    let raw_contract_class = std::fs::read_to_string(path).unwrap_or_else(|err| {
        eprintln!("Error reading Sierra file: {}", err);
        process::exit(1);
    });
    let contract_class: ContractClass = serde_json::from_str(&raw_contract_class)
        .unwrap_or_else(|err| {
            eprintln!("Error deserializing Sierra file into contract class: {}", err);
            process::exit(1);
        });
    contract_class.extract_sierra_program().unwrap_or_else(|err| {
        eprintln!("Error extracting Sierra program from contract class: {}", err);
        process::exit(1);
    })
}

#[cfg(test)]
fn test_compilation() {
    let sierra_program = load_sierra_program_from_file(&PathBuf::from("test.sierra"));
    let mut contract_executor = AotContractExecutor::new(&sierra_program, OptLevel::default())
        .unwrap_or_else(|err| {
            eprintln!("Error compiling Sierra program: {}", err);
            process::exit(1);
        });
    contract_executor.save("test.so".to_string()).unwrap_or_else(|err| {
        eprintln!("Error saving compiled program: {}", err);
        process::exit(1);
    });
    let loaded_executor =
        AotContractExecutor::load(Path::new("test.so")).unwrap_or_else(|err| {
            eprintln!("Error loading compiled program: {}", err);
            process::exit(1);
        });
    assert_eq!(contract_executor, loaded_executor);
}


