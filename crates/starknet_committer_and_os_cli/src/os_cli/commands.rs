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
use starknet_os::hint_processor::aggregator_hint_processor::AggregatorInput;
use starknet_os::io::os_input::{OsBlockInput, OsHints, StarknetOsInput};
use starknet_os::io::os_output::{StarknetAggregatorRunnerOutput, StarknetOsRunnerOutput};
use starknet_os::runner::{run_aggregator, run_os_stateless};
use starknet_types_core::felt::Felt;
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::reload::Handle;
use tracing_subscriber::Registry;

use crate::os_cli::run_os_cli::{AggregatorCliOutput, OsCliOutput, ProgramToDump};
use crate::shared_utils::read::{load_input, write_to_file};

#[derive(Deserialize, Debug)]
/// Input to the os runner.
pub(crate) struct OsCliInput {
    pub layout: LayoutName,
    pub os_hints: OsHints,
    pub cairo_pie_zip_path: String,
    pub raw_os_output_path: String,
}

#[derive(Deserialize, Debug)]
/// Input to the aggregator runner.
pub(crate) struct AggregatorCliInput {
    layout: LayoutName,
    aggregator_input: AggregatorInput,
    cairo_pie_zip_path: String,
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
pub(crate) fn validate_os_input(os_input: &StarknetOsInput) {
    assert_eq!(
        os_input.os_block_inputs.len(),
        os_input.cached_state_inputs.len(),
        "The number of blocks and number of state inputs should be equal."
    );
    for os_block_input in os_input.os_block_inputs.iter() {
        validate_single_input(os_block_input);
    }
}

pub(crate) fn parse_and_run_os(
    input_path: String,
    output_path: String,
    log_filter_handle: Handle<LevelFilter, Registry>,
) {
    let OsCliInput { layout, os_hints, cairo_pie_zip_path, raw_os_output_path } =
        load_input(input_path);
    log_filter_handle
        .modify(|filter| *filter = os_hints.os_hints_config.log_level())
        .expect("Failed to set the log level.");

    validate_os_input(&os_hints.os_input);

    info!("Running OS...");
    let StarknetOsRunnerOutput {
        raw_os_output, cairo_pie, da_segment, metrics, unused_hints, ..
    } = run_os_stateless(layout, os_hints)
        .unwrap_or_else(|err| panic!("OS run failed. Error: {}", err));

    info!("Finished running OS. Serializing OS output...");
    serialize_runner_output(
        &OsCliOutput {
            additional_data: &cairo_pie.additional_data,
            da_segment,
            metrics: metrics.into(),
            unused_hints,
        },
        output_path,
        &cairo_pie,
        cairo_pie_zip_path,
        Some(&raw_os_output),
        Some(raw_os_output_path),
    );
    info!("OS program ran successfully.");
}

pub(crate) fn parse_and_run_aggregator(
    input_path: String,
    output_path: String,
    log_filter_handle: Handle<LevelFilter, Registry>,
) {
    let AggregatorCliInput { layout, aggregator_input, cairo_pie_zip_path } =
        load_input(input_path);
    // TODO(Aner): Validate the aggregator input.
    log_filter_handle
        .modify(|filter| *filter = aggregator_input.log_level())
        .expect("Failed to set the log level.");

    let StarknetAggregatorRunnerOutput { cairo_pie, unused_hints, .. } =
        run_aggregator(layout, aggregator_input)
            .unwrap_or_else(|err| panic!("Aggregator run failed. Error: {}", err));
    serialize_runner_output(
        &AggregatorCliOutput { unused_hints },
        output_path,
        &cairo_pie,
        cairo_pie_zip_path,
        None,
        None,
    );
    info!("Aggregator program ran successfully.");
}

fn serialize_runner_output<T: serde::Serialize>(
    output: &T,
    output_path: String,
    cairo_pie: &CairoPie,
    cairo_pie_zip_path: String,
    raw_program_output: Option<&Vec<Felt>>,
    raw_program_output_path: Option<String>,
) {
    write_to_file(&output_path, output);
    let merge_extra_segments = true;
    info!("Writing Cairo Pie to zip file.");
    cairo_pie
        .write_zip_file(Path::new(&cairo_pie_zip_path), merge_extra_segments)
        .unwrap_or_else(|err| panic!("Failed to write cairo pie. Error: {}", err));
    info!(
        "Finished writing Cairo Pie to zip file. Zip file size: {} KB.",
        match std::fs::metadata(&cairo_pie_zip_path) {
            Err(_) => String::from("UNKNOWN"),
            Ok(meta) => format!("{}", meta.len() / 1024),
        }
    );

    if let Some(raw_program_output) = raw_program_output {
        let raw_program_output_path =
            raw_program_output_path.expect("Raw program output path should be provided.");
        info!("Writing raw program output to file.");
        write_to_file(&raw_program_output_path, raw_program_output);
        info!(
            "Finished writing raw program output to file. File size: {} KB.",
            match std::fs::metadata(&raw_program_output_path) {
                Err(_) => String::from("UNKNOWN"),
                Ok(meta) => format!("{}", meta.len() / 1024),
            }
        );
    } else {
        info!("No raw OS output to write.");
    }
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
