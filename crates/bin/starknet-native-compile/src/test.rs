use cairo_native::executor::AotContractExecutor;
use cairo_native::OptLevel;
use std::process;

use std::path::{Path, PathBuf};

use crate::utils::load_sierra_program_from_file;

#[test]
fn test_save_and_load_contract() {
    let sierra_program =
        load_sierra_program_from_file(&PathBuf::from("test_files/faulty_account.sierra.json"));
    let mut contract_executor = AotContractExecutor::new(&sierra_program, OptLevel::default())
        .unwrap_or_else(|err| {
            eprintln!("Error compiling Sierra program: {}", err);
            process::exit(1);
        });
    contract_executor.save("./target/test_output.so").unwrap_or_else(|err| {
        eprintln!("Error saving compiled program: {}", err);
        process::exit(1);
    });
    AotContractExecutor::load(Path::new("./target/test_output.so")).unwrap_or_else(|err| {
        eprintln!("Error loading compiled program: {}", err);
        process::exit(1);
    });
}
