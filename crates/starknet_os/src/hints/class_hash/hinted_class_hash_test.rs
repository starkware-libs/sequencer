use std::env::current_dir;
use std::fs::File;
use std::io::Read;

use rstest::rstest;
use starknet_api::deprecated_contract_class::ContractClass;
use starknet_types_core::felt::Felt;

use crate::hints::class_hash::hinted_class_hash::{
    add_backward_compatibility_space,
    compute_cairo_hinted_class_hash,
};

#[track_caller]
fn hinted_hash_from_file(file_path: &str) -> Felt {
    let mut file =
        File::open(file_path).unwrap_or_else(|_| panic!("Unable to open file {file_path}"));
    let mut data = String::new();
    file.read_to_string(&mut data).unwrap_or_else(|_| panic!("Unable to read file {file_path}"));
    let contract_class: ContractClass =
        serde_json::from_str(&data).expect("JSON was not well-formatted");
    compute_cairo_hinted_class_hash(&contract_class).expect("Failed to compute class hash")
}

// The contract and the expected hash are taken from the python side.
#[test]
fn test_compute_cairo_hinted_class_hash() {
    let contract_path = current_dir().unwrap().join("resources/test_contract.json");
    let computed_hash = hinted_hash_from_file(contract_path.to_str().unwrap());
    let expected_hash = Felt::from_hex_unchecked(
        "0x3D64E035186B556B0B88C52684FDDF6A9251944E763DCCA6637159C9FBC2D66",
    );
    assert_eq!(computed_hash, expected_hash, "Computed hash does not match expected hash");
}

#[rstest]
#[case::empty_tracking_data_and_scopes(
    "deprecated_proxy_original.json",
    "deprecated_proxy_reserialized.json"
)]
fn test_hinted_hash_equivalence(#[case] contract_0: &str, #[case] contract_1: &str) {
    let hash_0 = hinted_hash_from_file(
        current_dir().unwrap().join(format!("resources/{contract_0}")).to_str().unwrap(),
    );
    let hash_1 = hinted_hash_from_file(
        current_dir().unwrap().join(format!("resources/{contract_1}")).to_str().unwrap(),
    );
    assert_eq!(hash_0, hash_1, "{contract_0} and {contract_1} hinted hashes do not match.");
}

#[rstest]
#[case::basic("\"cairo_type\": \"(a: felt)\"", "\"cairo_type\": \"(a : felt)\"")]
#[case::basic_ptr("\"cairo_type\": \"(a: felt*)\"", "\"cairo_type\": \"(a : felt*)\"")]
#[case::two_tuple(
    "\"cairo_type\": \"(a: felt, b: felt)\"",
    "\"cairo_type\": \"(a : felt, b : felt)\""
)]
#[case::nested_tuple(
    "\"cairo_type\": \"(a: felt, b: (felt, felt), c: felt)\"",
    "\"cairo_type\": \"(a : felt, b : (felt, felt), c : felt)\""
)]
#[case::empty_tuple("\"cairo_type\": \"()\"", "\"cairo_type\": \"()\"")]
fn test_add_backward_compatibility_space(#[case] input: &str, #[case] expected_result: &str) {
    let mut input = input.to_string();
    add_backward_compatibility_space(&mut input);
    assert_eq!(input, expected_result, "The result does not match the expected result");
}
