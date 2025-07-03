use apollo_starknet_os_program::OS_PROGRAM;
use blockifier::abi::constants::L1_TO_L2_MSG_HEADER_SIZE;
use cairo_vm::types::program::Program;
use starknet_api::core::L2_ADDRESS_UPPER_BOUND;
use starknet_committer::hash_function::hash::TreeHashFunctionImpl;
use starknet_types_core::felt::Felt;

use crate::hints::hint_implementation::kzg::utils::FIELD_ELEMENTS_PER_BLOB;
use crate::hints::vars::CairoStruct;
use crate::vm_utils::get_size_of_cairo_struct;

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

#[test]
fn test_contract_class_hash_version() {
    assert_eq!(
        get_from_program(
            &OS_PROGRAM,
            "starkware.starknet.core.os.state.commitment.CONTRACT_CLASS_LEAF_VERSION"
        ),
        Felt::from_hex(TreeHashFunctionImpl::CONTRACT_CLASS_LEAF_V0).unwrap()
    );
}

#[test]
fn test_l1_to_l2_message_header_size() {
    assert_eq!(
        get_size_of_cairo_struct(CairoStruct::L1ToL2MessageHeader, &*OS_PROGRAM).unwrap(),
        L1_TO_L2_MSG_HEADER_SIZE
    );
}
