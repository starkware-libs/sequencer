use std::collections::HashMap;
use std::sync::LazyLock;

use cairo_vm::serde::deserialize_program::{InputFile, Location};

use super::get_code_snippet_from_filemap;

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

#[test]
fn test_get_code_snippet() {
    let location = Location {
        end_line: 8,
        end_col: 68,
        input_file: InputFile {
            filename: "crates/apollo_starknet_os_program/src/cairo/starkware/starknet/core/os/\
                       output.cairo"
                .to_string(),
        },
        parent_location: None,
        start_line: 8,
        start_col: 5,
    };
    let expected_snippet = r#"
    serialize_word(os_output_header.state_update_output.final_root);
    ^*************************************************************^"#;
    let snippet = get_code_snippet_from_filemap(&location, &TEST_CAIRO_FILES_MAP);
    assert_eq!(snippet.unwrap(), expected_snippet.strip_prefix("\n").unwrap());
}
