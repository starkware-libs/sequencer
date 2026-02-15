use apollo_starknet_os_program::{OS_PROGRAM, PROGRAM_HASHES};
use blockifier::abi::constants::{L1_TO_L2_MSG_HEADER_SIZE, L2_TO_L1_MSG_HEADER_SIZE};
use blockifier::blockifier_versioned_constants::VersionedConstants;
use starknet_api::block::StarknetVersion;
use starknet_api::contract_class::compiled_class_hash::COMPILED_CLASS_V1;
use starknet_api::core::{
    GLOBAL_STATE_VERSION,
    L2_ADDRESS_UPPER_BOUND,
    STARKNET_OS_CONFIG_HASH_VERSION,
};
use starknet_api::transaction::fields::{PROOF_VERSION, VIRTUAL_OS_OUTPUT_VERSION, VIRTUAL_SNOS};
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;
use starknet_committer::hash_function::hash::CONTRACT_CLASS_LEAF_V0;

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
        CONTRACT_CLASS_LEAF_V0
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

/// Asserts that the Rust VIRTUAL_OS_OUTPUT_VERSION constant matches the Cairo constant.
#[test]
fn test_virtual_os_output_version() {
    assert_eq!(
        Const::VirtualOsOutputVersion.fetch_from_os_program().unwrap(),
        VIRTUAL_OS_OUTPUT_VERSION
    );
}

/// Asserts that the Rust VIRTUAL_SNOS constant matches the Cairo constant.
#[test]
fn test_virtual_snos() {
    assert_eq!(Const::VirtualSnos.fetch_from_os_program().unwrap(), VIRTUAL_SNOS);
}

/// Asserts that the Rust PROOF_VERSION constant matches the Cairo constant.
#[test]
fn test_proof_version() {
    assert_eq!(Const::ProofVersion.fetch_from_os_program().unwrap(), PROOF_VERSION);
}

/// Asserts that the Rust STARKNET_OS_CONFIG_HASH_VERSION constant matches the Cairo constant.
#[test]
fn test_starknet_os_config_hash_version() {
    assert_eq!(
        Const::StarknetOsConfigVersion.fetch_from_os_program().unwrap(),
        STARKNET_OS_CONFIG_HASH_VERSION
    );
}

/// Verifies that the virtual OS program hash from PROGRAM_HASHES is in the list of
/// allowed virtual OS program hashes in the latest versioned constants.
#[test]
fn test_virtual_os_program_hash_is_allowed() {
    let virtual_os_hash = PROGRAM_HASHES.virtual_os;

    // Get the latest versioned constants
    let latest_constants = VersionedConstants::get(&StarknetVersion::LATEST).unwrap();

    // Check if the virtual OS program hash is in the allowed list
    let allowed_hashes = &latest_constants.os_constants.allowed_virtual_os_program_hashes;

    assert!(
        allowed_hashes.contains(&virtual_os_hash),
        "Virtual OS program hash {:#x} is not in the allowed list: {:?}",
        virtual_os_hash,
        allowed_hashes.iter().map(|h| format!("{:#x}", h)).collect::<Vec<_>>()
    );
}
