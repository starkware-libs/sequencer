use apollo_starknet_os_program::OS_PROGRAM;
use cairo_vm::types::program::Program;
use starknet_api::core::L2_ADDRESS_UPPER_BOUND;
use starknet_types_core::felt::Felt;

fn get_from_program(program: &Program, const_path: &str) -> Felt {
    program
        .constants
        .get(const_path)
        .cloned()
        .unwrap_or_else(|| panic!("Constant {const_path} not found in the program."))
}

#[test]
fn test_l2_address_bound() {
    assert_eq!(
        Felt::from(*L2_ADDRESS_UPPER_BOUND),
        get_from_program(&OS_PROGRAM, "starkware.starknet.common.storage.ADDR_BOUND")
    );
}
