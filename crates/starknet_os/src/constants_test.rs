use apollo_starknet_os_program::OS_PROGRAM;
use blockifier::abi::constants::{L1_TO_L2_MSG_HEADER_SIZE, L2_TO_L1_MSG_HEADER_SIZE};
use cairo_vm::types::program::Program;
use starknet_api::contract_class::compiled_class_hash::COMPILED_CLASS_V1;
use starknet_api::core::L2_ADDRESS_UPPER_BOUND;
use starknet_committer::hash_function::hash::TreeHashFunctionImpl;
use starknet_types_core::felt::Felt;

use crate::hints::hint_implementation::kzg::utils::FIELD_ELEMENTS_PER_BLOB;
use crate::hints::vars::{CairoStruct, Const};
use crate::io::os_output::{GLOBAL_STATE_VERSION, STARKNET_OS_CONFIG_HASH_VERSION};
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
fn test_global_state_version() {
    assert_eq!(
        get_from_program(
            &OS_PROGRAM,
            "starkware.starknet.core.os.state.commitment.GLOBAL_STATE_VERSION"
        ),
        GLOBAL_STATE_VERSION
    );
}

#[test]
fn test_os_config_hash_version() {
    assert_eq!(
        get_from_program(
            &OS_PROGRAM,
            "starkware.starknet.core.os.os_config.os_config.STARKNET_OS_CONFIG_VERSION"
        ),
        STARKNET_OS_CONFIG_HASH_VERSION
    );
}

#[test]
fn test_l1_to_l2_message_header_size() {
    assert_eq!(
        get_size_of_cairo_struct(CairoStruct::L1ToL2MessageHeader, &*OS_PROGRAM).unwrap(),
        L1_TO_L2_MSG_HEADER_SIZE
    );
}

#[test]
fn test_l2_to_l1_message_header_size() {
    assert_eq!(
        get_size_of_cairo_struct(CairoStruct::L2ToL1MessageHeader, &*OS_PROGRAM).unwrap(),
        L2_TO_L1_MSG_HEADER_SIZE
    );
}

#[test]
fn test_compiled_class_version() {
    assert_eq!(
        get_from_program(&OS_PROGRAM, Const::CompiledClassVersion.into()),
        *COMPILED_CLASS_V1
    );
}
