use pretty_assertions::assert_eq;
use starknet_crypto::Felt;

use crate::compression_utils::{compress_and_encode, decode_and_decompress};
use crate::test_utils::read_json_file;

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
    let expected_value: Vec<Felt> =
        serde_json::from_value(read_json_file("sierra_program.json")).unwrap();
    let value: Vec<Felt> = decode_and_decompress(&sierra_program_base64).unwrap();
    assert_eq!(value, expected_value);
}
