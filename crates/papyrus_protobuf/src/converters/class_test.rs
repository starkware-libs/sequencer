use papyrus_test_utils::{get_rng, GetTestInstance};
use starknet_api::deprecated_contract_class::ContractClass;

use crate::protobuf::Cairo0Class;
#[test]
fn convert_cairo_0_class_to_protobuf_and_back() {
    let expected_cairo_0_class = ContractClass::get_test_instance(&mut get_rng());
    let protobuf_class: Cairo0Class = expected_cairo_0_class.clone().into();
    let cairo_0_class: ContractClass = protobuf_class.try_into().unwrap();
    assert_eq!(cairo_0_class, expected_cairo_0_class);
}

// TODO: Add test for cairo 1 class conversion.
