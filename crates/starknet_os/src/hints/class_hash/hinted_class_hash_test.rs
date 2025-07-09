use std::env::current_dir;
use std::fs::File;
use std::io::Read;

use rstest::rstest;
use starknet_api::deprecated_contract_class::ContractClass;
use starknet_types_core::felt::Felt;

use crate::hints::class_hash::hinted_class_hash::compute_cairo_hinted_class_hash;

// The contract and the expected hash are taken from the python side.
#[rstest]
#[case(
    "resources/test_contract.json",
    Felt::from_hex_unchecked("0x3D64E035186B556B0B88C52684FDDF6A9251944E763DCCA6637159C9FBC2D66",)
)]
#[case(
    "resources/account.json",
    Felt::from_hex_unchecked("0xBFE8D78D97512C3C321FAA6FC578E3DBBC6ECAD0F2948087C77187A66BD1B",)
)]
#[case(
    "resources/deprecated_proxy.json",
    Felt::from_hex_unchecked("0x3F256EA66406A20C9E1C6A7DA6BD5443923200291A832DAF5F111D5357B469E",)
)]
fn test_compute_cairo_hinted_class_hash(
    #[case] contract_path_string: &str,
    #[case] expected_hash: Felt,
) {
    let contract_path = current_dir().unwrap().join(contract_path_string);
    let mut file = File::open(&contract_path)
        .unwrap_or_else(|_| panic!("Unable to open file {contract_path:?}"));
    let mut data = String::new();
    file.read_to_string(&mut data)
        .unwrap_or_else(|_| panic!("Unable to read file {contract_path:?}"));

    let contract_class: ContractClass =
        serde_json::from_str(&data).expect("JSON was not well-formatted");
    let computed_hash =
        compute_cairo_hinted_class_hash(&contract_class).expect("Failed to compute class hash");

    assert_eq!(computed_hash, expected_hash, "Computed hash does not match expected hash");
}
