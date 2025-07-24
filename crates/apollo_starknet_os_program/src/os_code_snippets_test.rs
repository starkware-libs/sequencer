use cairo_vm::serde::deserialize_program::{InputFile, Location};

use super::get_code_snippet;

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
    let expected_snippet = "    serialize_word(os_output_header.state_update_output.final_root);
    ^*************************************************************^";
    let snippet = get_code_snippet(&location);
    assert_eq!(snippet.unwrap(), expected_snippet);
}
