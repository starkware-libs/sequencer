use pretty_assertions::assert_eq;
use starknet_api::test_utils::read_json_file;

use super::{compress_and_encode, decode_and_decompress};

#[test]
fn compress_and_encode_hardcoded_value() {
    let sierra_program = read_json_file("sierra_program.json");
    let expected_value = read_json_file("sierra_program_base64.json").as_str().unwrap().to_owned();
    let value = compress_and_encode(sierra_program).unwrap();
    assert_eq!(value, expected_value);
}

#[test]
fn decode_and_decompress_hardcoded_value() {
    let sierra_program_base64 =
        read_json_file("sierra_program_base64.json").as_str().unwrap().to_owned();
    let expected_value = read_json_file("sierra_program.json");
    let value = decode_and_decompress(&sierra_program_base64).unwrap();
    assert_eq!(value, expected_value);
}
