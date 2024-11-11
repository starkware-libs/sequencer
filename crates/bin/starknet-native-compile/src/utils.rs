use cairo_lang_starknet_classes::contract_class::ContractClass;

use cairo_lang_sierra::program::Program;

use std::path::PathBuf;
use std::process;

pub(crate) fn load_sierra_program_from_file(path: &PathBuf) -> Program {
    let raw_contract_class = std::fs::read_to_string(path).unwrap_or_else(|err| {
        eprintln!("Error reading Sierra file: {}", err);
        process::exit(1);
    });
    let contract_class: ContractClass =
        serde_json::from_str(&raw_contract_class).unwrap_or_else(|err| {
            eprintln!("Error deserializing Sierra file into contract class: {}", err);
            process::exit(1);
        });
    contract_class.extract_sierra_program().unwrap_or_else(|err| {
        eprintln!("Error extracting Sierra program from contract class: {}", err);
        process::exit(1);
    })
}
