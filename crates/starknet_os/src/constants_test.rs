use apollo_starknet_os_program::OS_PROGRAM;
use cairo_vm::types::program::Program;
use starknet_api::core::L2_ADDRESS_UPPER_BOUND;
use starknet_types_core::felt::Felt;

use crate::hints::hint_implementation::kzg::utils::FIELD_ELEMENTS_PER_BLOB;

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
        get_from_program(&OS_PROGRAM, "starkware.starknet.common.storage.ADDR_BOUND"),
        (*L2_ADDRESS_UPPER_BOUND).into()
    );
}

#[test]
fn test_blob_constants() {
    assert_eq!(
        get_from_program(
            &OS_PROGRAM,
            "starkware.starknet.core.os.data_availability.commitment.BLOB_LENGTH"
        ),
        FIELD_ELEMENTS_PER_BLOB.into()
    );
}
