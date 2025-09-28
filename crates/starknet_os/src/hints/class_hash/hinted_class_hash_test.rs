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
use crate::hints::hint_implementation::deprecated_compiled_class::class_hash::compute_deprecated_class_hash;

fn contract_class_from_file(file_path: &str) -> ContractClass {
    let mut file =
        File::open(file_path).unwrap_or_else(|_| panic!("Unable to open file {file_path}"));
    let mut data = String::new();
    file.read_to_string(&mut data).unwrap_or_else(|_| panic!("Unable to read file {file_path}"));
    serde_json::from_str(&data).expect("JSON was not well-formatted")
}

#[track_caller]
fn hinted_hash_from_file(file_path: &str) -> Felt {
    let contract_class = contract_class_from_file(file_path);
    compute_cairo_hinted_class_hash(&contract_class).expect("Failed to compute class hash")
}

// The contract and the expected hash are taken from the python side.
#[rstest]
#[case(
    "test_contract.json",
    "0x3D64E035186B556B0B88C52684FDDF6A9251944E763DCCA6637159C9FBC2D66",
    "0x4E98101A760A9917664DE57C6703BEA71CE5D107266E610197FD67B17F9257A"
)]
#[case(
    "account.json",
    "0xBFE8D78D97512C3C321FAA6FC578E3DBBC6ECAD0F2948087C77187A66BD1B",
    "0x569A49FA4AB178DA395C5A1502DBBCDF8D53BFCE7851B87FAD0822F7D760F0"
)]
#[case(
    "deprecated_proxy.json",
    "0x3F256EA66406A20C9E1C6A7DA6BD5443923200291A832DAF5F111D5357B469E",
    "0xD0E183745E9DAE3E4E78A8FFEDCCE0903FC4900BEACE4E0ABF192D4C202DA3"
)]
#[case(
    "nested_tuple_value_contract.json",
    "0x24CB45DE406D17148C9C3DDEC7BB80ADBABFFBB64F9D7A521AD3F83285444CD",
    "0x6DC10E7703C1B63E0B5A4E8E7842293D3255FD4E53D4E730ADF435C3DFFABB"
)]
fn test_compute_cairo_class_hash(
    #[case] contract_path_string: &str,
    #[case] expected_hinted_hash_hex: &str,
    #[case] expected_hash_hex: &str,
) {
    let expected_hinted_hash = Felt::from_hex_unchecked(expected_hinted_hash_hex);
    let expected_hash = Felt::from_hex_unchecked(expected_hash_hex);
    let contract_path = current_dir().unwrap().join(format!("resources/{contract_path_string}"));
    let contract_class = contract_class_from_file(contract_path.to_str().unwrap());
    let computed_hinted_hash =
        compute_cairo_hinted_class_hash(&contract_class).expect("Failed to compute class hash");
    let computed_hash =
        compute_deprecated_class_hash(&contract_class).expect("Failed to compute class hash");
    assert_eq!(
        computed_hinted_hash, expected_hinted_hash,
        "Computed hinted hash does not match expected hinted hash"
    );
    assert_eq!(computed_hash, expected_hash, "Computed hash does not match expected hash");
}

#[rstest]
#[case::empty_tracking_data_and_scopes(
    "deprecated_proxy.json",
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
