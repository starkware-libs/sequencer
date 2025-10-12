use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use starknet_api::contract_class::{ContractClass, SierraVersion};

use crate::{RawClass, RawExecutableClass};

#[test]
fn compact_serialization() {
    const EXPECTED: &str = "{\"V1\":[{\"bytecode\":[\"0x1\",\"0x1\",\"0x1\"],\"compiler_version\":\
                            \"\",\"entry_points_by_type\":{\"CONSTRUCTOR\":[],\"EXTERNAL\":[],\"\
                            L1_HANDLER\":[]},\"hints\":[],\"prime\":\"0x0\"},\"1.7.0\"]}";
    let raw_executable_class =
        RawExecutableClass::try_from(ContractClass::test_casm_contract_class()).unwrap();
    let serialized = serde_json::to_string(&raw_executable_class.0).unwrap();

    assert_eq!(serialized, EXPECTED);
    assert_eq!(raw_executable_class.size().unwrap(), EXPECTED.len());
}

#[test]
fn consistent_serialization_size() {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));

    let sierra_class = test_contract.get_sierra();
    let sierra_class_length = serde_json::to_string(&sierra_class).unwrap().len();

    let raw_sierra_class: RawClass = sierra_class.try_into().unwrap();
    let raw_sierra_class_size = raw_sierra_class.size().unwrap();

    assert_eq!(raw_sierra_class_size, sierra_class_length);

    let casm_class = ContractClass::V1((
        serde_json::from_str(&test_contract.get_raw_class()).unwrap(),
        SierraVersion::LATEST,
    ));
    let casm_class_length = serde_json::to_string(&casm_class).unwrap().len();

    let raw_casm_class = RawExecutableClass::try_from(casm_class).unwrap();
    let raw_casm_class_size = raw_casm_class.size().unwrap();

    assert_eq!(raw_casm_class_size, casm_class_length);
}
