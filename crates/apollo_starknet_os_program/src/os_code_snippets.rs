#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::sync::LazyLock;

use cairo_vm::serde::deserialize_program::Location;
use cairo_vm::vm::errors::vm_exception::{get_error_attr_value, get_location};
use cairo_vm::vm::runners::cairo_runner::CairoRunner;

use crate::CAIRO_FILES_MAP;

#[cfg(test)]
#[path = "os_code_snippets_test.rs"]
pub mod test;

#[cfg(test)]
static TEST_CAIRO_FILES_MAP: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    HashMap::from([(
        "starkware/starknet/core/os/output.cairo".to_string(),
        "// Serializes to output the constant-sized execution info needed for the L1 state update;
// for example, state roots and config hash.
func serialize_output_header{output_ptr: felt*}(os_output_header: OsOutputHeader*) {
    // Serialize program output.

    // Serialize roots.
    serialize_word(os_output_header.state_update_output.initial_root);
    serialize_word(os_output_header.state_update_output.final_root);
    serialize_word(os_output_header.prev_block_number);
    serialize_word(os_output_header.new_block_number);
    serialize_word(os_output_header.prev_block_hash);
    serialize_word(os_output_header.new_block_hash);
    serialize_word(os_output_header.os_program_hash);
    serialize_word(os_output_header.starknet_os_config_hash);
    serialize_word(os_output_header.use_kzg_da);
    serialize_word(os_output_header.full_output);

    return ();
}"
        .to_string(),
    )])
});

/// Gets a code snippet from an OS file at a specific location.
fn get_code_snippet(location: &Location) -> Option<String> {
    let path = location.input_file.filename.split_once("cairo/").map(|(_, rest)| rest)?;
    let file_bytes = get_file_content(path)?.as_bytes();

    Some(location.get_location_marks(file_bytes).to_string())
}

fn get_file_content(path: &str) -> Option<&'static str> {
    #[cfg(test)]
    {
        TEST_CAIRO_FILES_MAP.get(path).map(String::as_str)
    }

    #[cfg(not(test))]
    {
        CAIRO_FILES_MAP.get(path).map(String::as_str)
    }
}

/// Adds code snippets to the traceback of a VM exception.
#[allow(dead_code)]
pub fn get_traceback_with_code_snippet(runner: &CairoRunner) -> Option<String> {
    let traceback: String = runner
        .vm
        .get_traceback_entries()
        .iter()
        .map(|(_fp, traceback_pc)| {
            let unknown_frame = format!("Unknown location (pc={traceback_pc})\n");
            if traceback_pc.segment_index != 0 {
                return unknown_frame;
            }
            if let Some(ref attr) = get_error_attr_value(traceback_pc.offset, runner) {
                return format!("{attr}\n");
            }
            match &get_location(traceback_pc.offset, runner, None) {
                Some(location) => {
                    let location_str = location.to_string(&format!("(pc={traceback_pc})"));
                    let snippet = get_code_snippet(location)
                        .unwrap_or_else(|| "Code snippet not found.".to_string());
                    format!("{location_str}\n{snippet}\n")
                }
                None => unknown_frame,
            }
        })
        .collect();

    (!traceback.is_empty())
        .then(|| format!("Cairo traceback (most recent call last):\n{traceback}"))
}
