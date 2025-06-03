use std::fs;
use std::path::Path;

use apollo_starknet_os_program::test_programs::ALIASES_TEST_BYTES;
use apollo_starknet_os_program::{
    AGGREGATOR_PROGRAM_BYTES,
    CAIRO_FILES_MAP,
    OS_PROGRAM_BYTES,
    PROGRAM_HASHES,
};
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::vm::runners::cairo_pie::CairoPie;
use rand_distr::num_traits::Zero;
use serde::Deserialize;
use starknet_api::contract_class::ContractClass;
use starknet_api::executable_transaction::{AccountTransaction, Transaction};
use starknet_os::io::os_input::{OsBlockInput, OsHints, StarknetOsInput};
use starknet_os::io::os_output::StarknetOsRunnerOutput;
use starknet_os::runner::run_os_stateless;
use tracing::info;

use crate::os_cli::run_os_cli::{OsCliOutput, ProgramToDump};
use crate::shared_utils::read::{load_input, write_to_file};

#[derive(Deserialize, Debug)]
/// Input to the os runner.
pub(crate) struct Input {
    // A path to a compiled program that its hint set should be a subset of those defined in
    // starknet-os.
    pub compiled_os_path: String,
    pub layout: LayoutName,
    pub os_hints: OsHints,
    pub cairo_pie_zip_path: String,
}

/// Validate a single os_block_input.
fn validate_single_input(os_block_input: &OsBlockInput) {
    assert!(
        os_block_input.transactions.len() == os_block_input.tx_execution_infos.len(),
        "The number of transactions and execution infos should be equal"
    );

    // The CasmContractClass in Declare transactions should hold invalid data to mark it should not
    // be used.
    assert!(
        os_block_input
            .transactions
            .iter()
            .filter_map(|tx| {
                if let Transaction::Account(AccountTransaction::Declare(declare_tx)) = tx {
                    Some(&declare_tx.class_info.contract_class)
                } else {
                    None
                }
            })
            .all(|contract_class| match contract_class {
                ContractClass::V0(_) => false,
                ContractClass::V1((CasmContractClass { prime, .. }, _)) => prime.is_zero(),
            }),
        "All declare transactions should be of V1 and should have contract class with prime=0"
    );
    let block_number = os_block_input.block_info.block_number;
    info!("Parsed OS input successfully for block number: {}", block_number);
}

/// Validate a list of os_block_input.
pub fn validate_input(os_input: &StarknetOsInput) {
    assert_eq!(
        os_input.os_block_inputs.len(),
        os_input.cached_state_inputs.len(),
        "The number of blocks and number of state inputs should be equal."
    );
    for os_block_input in os_input.os_block_inputs.iter() {
        validate_single_input(os_block_input);
    }
}

pub fn parse_and_run_os(input_path: String, output_path: String) {
    let Input { compiled_os_path, layout, os_hints, cairo_pie_zip_path } = load_input(input_path);
    validate_input(&os_hints.os_input);

    // Load the compiled_os from the compiled_os_path.
    let compiled_os =
        fs::read(Path::new(&compiled_os_path)).expect("Failed to read compiled_os file");

    let StarknetOsRunnerOutput { os_output, cairo_pie, unused_hints } =
        run_os_stateless(&compiled_os, layout, os_hints)
            .unwrap_or_else(|err| panic!("OS run failed. Error: {}", err));
    serialize_os_runner_output(
        &OsCliOutput { os_output, unused_hints },
        output_path,
        &cairo_pie,
        cairo_pie_zip_path,
    );
    info!("OS program ran successfully.");
}

pub(crate) fn serialize_os_runner_output(
    output: &OsCliOutput,
    output_path: String,
    cairo_pie: &CairoPie,
    cairo_pie_zip_path: String,
) {
    write_to_file(&output_path, output);
    let merge_extra_segments = true;
    cairo_pie
        .write_zip_file(Path::new(&cairo_pie_zip_path), merge_extra_segments)
        .unwrap_or_else(|err| panic!("Failed to write cairo pie. Error: {}", err));
}

pub(crate) fn dump_source_files(output_path: String) {
    write_to_file(&output_path, &*CAIRO_FILES_MAP);
}

pub(crate) fn dump_program(output_path: String, program: ProgramToDump) {
    let bytes = match program {
        ProgramToDump::Aggregator => AGGREGATOR_PROGRAM_BYTES,
        ProgramToDump::AliasesTest => ALIASES_TEST_BYTES,
        ProgramToDump::Os => OS_PROGRAM_BYTES,
    };
    // Dumping the `Program` struct won't work - it is not deserializable via cairo-lang's Program
    // class. JSONify the raw bytes instead.
    let program_json = serde_json::from_slice::<serde_json::Value>(bytes)
        .expect("Program bytes are JSON-serializable.");
    write_to_file(&output_path, &program_json);
}

pub(crate) fn dump_program_hashes(output_path: String) {
    write_to_file(&output_path, &*PROGRAM_HASHES);
}
