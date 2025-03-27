use std::fs;
use std::path::Path;

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_vm::types::layout_name::LayoutName;
use rand_distr::num_traits::Zero;
use serde::Deserialize;
use starknet_api::contract_class::ContractClass;
use starknet_api::executable_transaction::{AccountTransaction, Transaction};
use starknet_os::io::os_input::{CachedStateInput, OsHints, StarknetOsInput};
use starknet_os::runner::run_os_stateless;
use tracing::info;

use crate::shared_utils::read::{load_input, write_to_file};

#[derive(Deserialize, Debug)]
/// Input to the os runner.
pub(crate) struct Input {
    // A path to a compiled program that its hint set should be a subset of those defined in
    // starknet-os.
    pub compiled_os_path: String,
    pub layout: LayoutName,
    pub os_hints: OsHints,
    pub cached_state_input: CachedStateInput,
}

/// Validate the os_input.
pub fn validate_input(os_input: &StarknetOsInput) {
    assert!(
        os_input.transactions.len() == os_input._tx_execution_infos.len(),
        "The number of transactions and execution infos should be equal"
    );

    // The CasmContractClass in Declare transactions should hold invalid data to mark it should not
    // be used.
    assert!(
        os_input
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
}

pub fn parse_and_run_os(input_path: String, output_path: String) {
    let Input { compiled_os_path, layout, os_hints, cached_state_input } = load_input(input_path);
    validate_input(&os_hints.os_input);
    let block_number = os_hints.os_input.block_info.block_number;
    info!("Parsed OS input successfully for block number: {}", block_number);

    // Load the compiled_os from the compiled_os_path.
    let compiled_os =
        fs::read(Path::new(&compiled_os_path)).expect("Failed to read compiled_os file");

    let output = run_os_stateless(&compiled_os, layout, os_hints, cached_state_input)
        .unwrap_or_else(|err| {
            panic!("OS run failed on block number: {}. Error: {}", block_number, err)
        });
    write_to_file(&output_path, &output);
    info!("OS program ran successfully on block number: {}", block_number);
}
