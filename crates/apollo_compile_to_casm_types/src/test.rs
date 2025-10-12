use starknet_api::contract_class::ContractClass;

use crate::RawExecutableClass;

#[test]
fn test_contract_class_size() {
    const EXPECTED: &str = "{\"V1\":[{\"bytecode\":[\"0x1\",\"0x1\",\"0x1\"],\"compiler_version\":\
                            \"\",\"entry_points_by_type\":{\"CONSTRUCTOR\":[],\"EXTERNAL\":[],\"\
                            L1_HANDLER\":[]},\"hints\":[],\"prime\":\"0x0\"},\"1.7.0\"]}";
    let raw_executable_class =
        RawExecutableClass::try_from(ContractClass::test_casm_contract_class()).unwrap();
    let serialized = serde_json::to_string(&raw_executable_class.0).unwrap();

    assert_eq!(serialized, EXPECTED);
    assert_eq!(raw_executable_class.size().unwrap(), EXPECTED.len());
}
