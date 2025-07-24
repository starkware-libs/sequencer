#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::sync::LazyLock;

use cairo_vm::serde::deserialize_program::Location;

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

#[allow(dead_code)]
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
