use std::fs;
use std::path::Path;

use cairo_vm::types::layout_name::LayoutName;
use serde::Deserialize;
use starknet_os::io::os_input::StarknetOsInput;
use starknet_os::runner::run_os_stateless;
use tracing::info;

use crate::shared_utils::read::load_input;

#[derive(Deserialize, Debug)]
/// Input to the os runner.
pub(crate) struct Input {
    // A path to a compiled program that its hint set should be a subset of those defined in
    // starknet-os.
    pub compiled_os_path: String,
    pub layout: LayoutName,
    pub os_input: StarknetOsInput,
}

pub fn parse_and_run_os(input_path: String, _output_path: String) {
    let Input { compiled_os_path, layout, os_input } = load_input(input_path);
    info!("Parsed OS input successfully for block number: {}", os_input.block_info.block_number);

    // Load the compiled_os from the compiled_os_path
    let compiled_os =
        fs::read(Path::new(&compiled_os_path)).expect("Failed to read compiled_os file");

    let _ = run_os_stateless(&compiled_os, layout, os_input);
}
