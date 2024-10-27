use papyrus_test_utils::{get_rng, GetTestInstance};
use starknet_api::deprecated_contract_class::ContractClass;

use crate::protobuf::Cairo0Class;
#[test]
fn test_deprecated_contract_class_to_cairo0class_conversion() {
    let expected_contract_class = ContractClass::get_test_instance(&mut get_rng());
    let expected_cairo0_class: Cairo0Class = expected_contract_class.clone().into();
    let contract_class: ContractClass = expected_cairo0_class.try_into().unwrap();
    assert_eq!(contract_class, expected_contract_class);
}
