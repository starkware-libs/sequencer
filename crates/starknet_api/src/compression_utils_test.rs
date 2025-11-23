use pretty_assertions::assert_eq;
use starknet_crypto::Felt;

use crate::compression_utils::{compress_and_encode, decode_and_decompress};
use crate::test_utils::read_json_file;

#[test]
fn compress_and_encode_hardcoded_value() {
    let value = compress_and_encode(&read_json_file::<_, serde_json::Value>("sierra_program.json"))
        .unwrap();
    let expected_value: String = read_json_file("sierra_program_base64.json");
    assert_eq!(value, expected_value);
}

#[test]
fn decode_and_decompress_hardcoded_value() {
    let sierra_program_base64: String = read_json_file("sierra_program_base64.json");
    let expected_value: Vec<Felt> = read_json_file("sierra_program.json");
    let value: Vec<Felt> = decode_and_decompress(&sierra_program_base64).unwrap();
    assert_eq!(value, expected_value);
}
