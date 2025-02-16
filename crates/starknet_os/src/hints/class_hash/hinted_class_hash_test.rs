use std::env::current_dir;
use std::fs::File;
use std::io::Read;

use starknet_types_core::felt::Felt;

use crate::hints::class_hash::hinted_class_hash::{
    compute_cairo_hinted_class_hash,
    CairoContractDefinition,
};

// The contract and the expected hash are taken from the python side.
#[test]
fn test_compute_cairo_hinted_class_hash() {
    let contract_path = current_dir().unwrap().join("resources/legacy_contract.json");
    let mut file = File::open(&contract_path)
        .unwrap_or_else(|_| panic!("Unable to open file {contract_path:?}"));
    let mut data = String::new();
    file.read_to_string(&mut data)
        .unwrap_or_else(|_| panic!("Unable to read file {contract_path:?}"));

    let contract_definition: CairoContractDefinition<'_> =
        serde_json::from_str(&data).expect("JSON was not well-formatted");
    let computed_hash = compute_cairo_hinted_class_hash(&contract_definition)
        .expect("Failed to compute class hash");

    let expected_hash = Felt::from_hex_unchecked(
        "0x1DBF36F651C9917E703BF6932FA4E866BFB6BCBFF18765F769CA9401C2CAF4F",
    );
    assert_eq!(computed_hash, expected_hash, "Computed hash does not match expected hash");
}
