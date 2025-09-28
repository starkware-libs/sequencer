use apollo_starknet_os_program::OS_PROGRAM;
use blockifier::abi::constants::{L1_TO_L2_MSG_HEADER_SIZE, L2_TO_L1_MSG_HEADER_SIZE};
use cairo_vm::types::program::Program;
use starknet_api::contract_class::compiled_class_hash::COMPILED_CLASS_V1;
use starknet_api::core::{GLOBAL_STATE_VERSION, L2_ADDRESS_UPPER_BOUND};
use starknet_committer::hash_function::hash::TreeHashFunctionImpl;
use starknet_types_core::felt::Felt;

use crate::hints::hint_implementation::kzg::utils::FIELD_ELEMENTS_PER_BLOB;
use crate::hints::vars::{CairoStruct, Const};
use crate::vm_utils::get_size_of_cairo_struct;

#[test]
fn test_l2_address_bound() {
    assert_eq!(Const::AddrBound.fetch_from_os_program().unwrap(), (*L2_ADDRESS_UPPER_BOUND).into());
}

/// Field elements per blob in the rust code is defined w.r.t.
/// [crate::hints::hint_implementation::kzg::utils::LOG2_FIELD_ELEMENTS_PER_BLOB], so it should be
/// tested.
#[test]
fn test_blob_constants() {
    assert_eq!(Const::BlobLength.fetch_from_os_program().unwrap(), FIELD_ELEMENTS_PER_BLOB.into());
}

#[test]
fn test_contract_class_hash_version() {
    assert_eq!(
        Const::ContractClassLeafVersion.fetch_from_os_program().unwrap(),
        Felt::from_hex(TreeHashFunctionImpl::CONTRACT_CLASS_LEAF_V0).unwrap()
    );
}

#[test]
fn test_global_state_version() {
    assert_eq!(Const::GlobalStateVersion.fetch_from_os_program().unwrap(), GLOBAL_STATE_VERSION);
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
    assert_eq!(Const::CompiledClassVersion.fetch_from_os_program().unwrap(), *COMPILED_CLASS_V1);
}
